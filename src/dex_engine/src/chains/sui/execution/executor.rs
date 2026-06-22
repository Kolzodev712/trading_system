//! Sign and submit DeepBook transactions.

use std::collections::HashMap;
use std::sync::Arc;

use rand::random;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use sui_transaction_builder::{ObjectInput, TransactionBuilder};
use tracing::debug;
use trading_core::venue::dex::types::{ExecutionResult, TxStatus};
use trading_core::venue::dex::{DexError, TradeCommand};

use super::super::config::DeepBookConfig;
use super::super::grpc::SuiGrpcClient;
use super::super::types::DeepBookPoolMeta;
use super::keys::SuiSigner;
use super::ptb::{amount_to_coin_units, parse_address, DeepBookPtbContext, CLOCK_OBJECT_ID};
use super::rpc_types::{coin_to_object_input, CoinRef};
use super::submit::finish_and_submit;

pub struct DeepBookExecutor {
    config: DeepBookConfig,
    signer: SuiSigner,
    grpc: SuiGrpcClient,
    pools: Arc<HashMap<smol_str::SmolStr, DeepBookPoolMeta>>,
    clock: ObjectInput,
}

impl DeepBookExecutor {
    pub async fn new(
        config: DeepBookConfig,
        grpc: SuiGrpcClient,
        pools: Arc<HashMap<smol_str::SmolStr, DeepBookPoolMeta>>,
    ) -> Result<Self, DexError> {
        let signer = SuiSigner::from_env(config.private_key_env.as_str())?;
        let shared = grpc.get_shared_object(CLOCK_OBJECT_ID).await?;
        let clock = ObjectInput::shared(
            shared.object_id,
            shared.initial_shared_version,
            shared.mutable,
        );
        debug!("DeepBook executor ready for address {}", signer.address());
        Ok(Self {
            config,
            signer,
            grpc,
            pools,
            clock,
        })
    }

    pub fn signer_address(&self) -> SmolStr {
        SmolStr::new(self.signer.address().to_string())
    }

    pub async fn execute(&self, command: TradeCommand) -> Result<ExecutionResult, DexError> {
        let digest = self.submit(command.clone()).await?;
        Ok(ExecutionResult {
            signature: digest,
            status: TxStatus::Pending,
            error: None,
        })
    }

    pub async fn poll_transaction(&self, digest: &str) -> Result<TxStatus, DexError> {
        self.grpc.get_transaction_status(digest).await
    }

    async fn submit(&self, command: TradeCommand) -> Result<SmolStr, DexError> {
        let package_id = parse_address(&self.config.deepbook_package_id()?)?;
        let mut builder = TransactionBuilder::new();
        match &command {
            TradeCommand::PlaceLimitOrder {
                pool_id,
                balance_manager_id,
                side,
                price,
                quantity,
                pay_fees_in_deep,
            } => {
                let ctx = self.ptb_context(package_id, pool_id, balance_manager_id)?;
                let client_order_id = random();
                ctx.append_place_limit_order(
                    &mut builder,
                    side.clone(),
                    *price,
                    *quantity,
                    *pay_fees_in_deep,
                    client_order_id,
                )?;
            }
            TradeCommand::CancelOrder {
                pool_id,
                balance_manager_id,
                order_id,
            } => {
                let ctx = self.ptb_context(package_id, pool_id, balance_manager_id)?;
                ctx.append_cancel_order(&mut builder, order_id.as_str())?;
            }
            TradeCommand::CancelAllOrders {
                pool_id,
                balance_manager_id,
            } => {
                let ctx = self.ptb_context(package_id, pool_id, balance_manager_id)?;
                ctx.append_cancel_all_orders(&mut builder)?;
            }
            TradeCommand::Deposit {
                balance_manager_id,
                coin_type,
                amount,
            } => {
                self.append_deposit(
                    &mut builder,
                    package_id,
                    balance_manager_id,
                    coin_type.as_str(),
                    *amount,
                )
                .await?;
            }
            TradeCommand::Withdraw {
                balance_manager_id,
                coin_type,
                amount,
            } => {
                let pool = self.first_pool()?;
                let ctx = DeepBookPtbContext {
                    package_id,
                    pool,
                    balance_manager_id: parse_address(balance_manager_id.as_str())?,
                    clock: self.clock.clone(),
                };
                ctx.append_withdraw(
                    &mut builder,
                    coin_type.as_str(),
                    *amount,
                    self.signer.address(),
                )?;
            }
            TradeCommand::Liquidate { .. } | TradeCommand::Swap { .. } => {
                return Err(DexError::UnsupportedCommand(
                    "Liquidate/Swap not supported on DeepBook".into(),
                ));
            }
        }
        finish_and_submit(&self.grpc, &self.signer, builder)
            .await
            .map(|r| r.digest)
    }

    async fn append_deposit(
        &self,
        builder: &mut TransactionBuilder,
        package_id: sui_sdk_types::Address,
        balance_manager_id: &SmolStr,
        coin_type: &str,
        amount: Decimal,
    ) -> Result<(), DexError> {
        let pool = self.first_pool()?;
        let ctx = DeepBookPtbContext {
            package_id,
            pool: pool.clone(),
            balance_manager_id: parse_address(balance_manager_id.as_str())?,
            clock: self.clock.clone(),
        };
        let owner = self.signer.address().to_string();
        let coins = self.grpc.get_coins(&owner, coin_type).await?;
        let coin = pick_funding_coin(&coins, amount, coin_type, &pool)?;
        ctx.append_deposit(builder, coin_type, amount, coin_to_object_input(coin))
    }

    fn ptb_context(
        &self,
        package_id: sui_sdk_types::Address,
        pool_id: &SmolStr,
        balance_manager_id: &SmolStr,
    ) -> Result<DeepBookPtbContext, DexError> {
        let pool = self.resolve_pool(pool_id)?;
        Ok(DeepBookPtbContext {
            package_id,
            pool,
            balance_manager_id: parse_address(balance_manager_id.as_str())?,
            clock: self.clock.clone(),
        })
    }

    fn resolve_pool(&self, pool_id: &SmolStr) -> Result<DeepBookPoolMeta, DexError> {
        if let Some(meta) = self.pools.get(pool_id.as_str()) {
            return Ok(meta.clone());
        }
        for meta in self.pools.values() {
            if meta.pool_id.as_str() == pool_id.as_str() {
                return Ok(meta.clone());
            }
        }
        Err(DexError::InvalidConfig(
            format!("unknown pool `{pool_id}` (not in configured pools)").into(),
        ))
    }

    fn first_pool(&self) -> Result<DeepBookPoolMeta, DexError> {
        self.pools
            .values()
            .next()
            .cloned()
            .ok_or_else(|| DexError::InvalidConfig("no pools loaded".into()))
    }
}

fn pick_funding_coin<'a>(
    coins: &'a [CoinRef],
    amount: Decimal,
    coin_type: &str,
    pool: &DeepBookPoolMeta,
) -> Result<&'a CoinRef, DexError> {
    let needed = amount_to_coin_units(amount, coin_type, pool)?;
    coins.iter().find(|c| c.balance >= needed).ok_or_else(|| {
        DexError::Transaction(
            format!("insufficient `{coin_type}` balance for deposit of {amount}").into(),
        )
    })
}
