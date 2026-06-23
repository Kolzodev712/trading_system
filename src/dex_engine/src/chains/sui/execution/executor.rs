//! Sign and submit DeepBook transactions.

use std::collections::HashMap;
use std::sync::Arc;

use rand::random;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use sui_sdk_types::Address;
use sui_transaction_builder::{ObjectInput, TransactionBuilder};
use tracing::debug;
use trading_core::venue::dex::types::{ExecutionResult, TxStatus};
use trading_core::venue::dex::{DexError, TradeCommand};

use super::super::config::DeepBookConfig;
use super::super::grpc::SuiGrpcClient;
use super::super::types::DeepBookPoolMeta;
use super::keys::SuiSigner;
use super::ptb::GAS_BUDGET;
use super::ptb::{
    amount_to_coin_units, coin_types_match, parse_address, DeepBookPtbContext, CLOCK_OBJECT_ID,
};
use super::rpc_types::{coin_to_object_input, pick_gas_coin, pick_sui_deposit_coins, CoinRef};
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
        let signer = config.signer()?;
        let shared = grpc.get_shared_object(CLOCK_OBJECT_ID).await?;
        let clock = ObjectInput::shared(shared.object_id, shared.initial_shared_version, false);
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
        let mut reserved_gas: Option<CoinRef> = None;
        match &command {
            TradeCommand::PlaceLimitOrder {
                pool_id,
                balance_manager_id,
                side,
                price,
                quantity,
                pay_fees_in_deep,
            } => {
                let ctx = self
                    .ptb_context(package_id, pool_id, balance_manager_id)
                    .await?;
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
                let ctx = self
                    .ptb_context(package_id, pool_id, balance_manager_id)
                    .await?;
                ctx.append_cancel_order(&mut builder, order_id.as_str())?;
            }
            TradeCommand::CancelAllOrders {
                pool_id,
                balance_manager_id,
            } => {
                let ctx = self
                    .ptb_context(package_id, pool_id, balance_manager_id)
                    .await?;
                ctx.append_cancel_all_orders(&mut builder)?;
            }
            TradeCommand::Deposit {
                balance_manager_id,
                coin_type,
                amount,
            } => {
                reserved_gas = Some(
                    self.append_deposit(
                        &mut builder,
                        package_id,
                        balance_manager_id,
                        coin_type.as_str(),
                        *amount,
                    )
                    .await?,
                );
            }
            TradeCommand::Withdraw {
                balance_manager_id,
                coin_type,
                amount,
            } => {
                let ctx = self
                    .ptb_context_for_balance_manager(package_id, balance_manager_id)
                    .await?;
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
        finish_and_submit(&self.grpc, &self.signer, builder, reserved_gas.as_ref())
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
    ) -> Result<CoinRef, DexError> {
        let pool = self.first_pool()?;
        let ctx = self
            .ptb_context_for_balance_manager(package_id, balance_manager_id)
            .await?;
        let owner = self.signer.address().to_string();
        let coins = self.grpc.get_coins(&owner, coin_type).await?;
        let needed = amount_to_coin_units(amount, coin_type, &pool)?;

        if coin_types_match(coin_type, super::rpc_types::SUI_COIN_TYPE) {
            let sui_coins = coins.clone();
            let (gas, fund) = pick_sui_deposit_coins(&sui_coins, needed, GAS_BUDGET)?;
            ctx.append_deposit(builder, coin_type, amount, coin_to_object_input(fund))?;
            return Ok(gas.clone());
        }

        let sui_coins = self.grpc.get_sui_coins(&owner).await?;
        let gas_coin = pick_gas_coin(&sui_coins)?.clone();
        let funding =
            pick_funding_coin(&coins, amount, coin_type, &pool, Some(gas_coin.object_id))?;
        ctx.append_deposit(builder, coin_type, amount, coin_to_object_input(funding))?;
        Ok(gas_coin)
    }

    async fn ptb_context(
        &self,
        package_id: sui_sdk_types::Address,
        pool_id: &SmolStr,
        balance_manager_id: &SmolStr,
    ) -> Result<DeepBookPtbContext, DexError> {
        let pool = self.resolve_pool(pool_id)?;
        let balance_manager = self
            .shared_object_input(balance_manager_id.as_str())
            .await?;
        let pool_object = self.shared_object_input(pool.pool_id.as_str()).await?;
        Ok(DeepBookPtbContext {
            package_id,
            pool,
            balance_manager,
            pool_object,
            clock: self.clock.clone(),
        })
    }

    async fn ptb_context_for_balance_manager(
        &self,
        package_id: sui_sdk_types::Address,
        balance_manager_id: &SmolStr,
    ) -> Result<DeepBookPtbContext, DexError> {
        let pool = self.first_pool()?;
        let balance_manager = self
            .shared_object_input(balance_manager_id.as_str())
            .await?;
        let pool_object = self.shared_object_input(pool.pool_id.as_str()).await?;
        Ok(DeepBookPtbContext {
            package_id,
            pool,
            balance_manager,
            pool_object,
            clock: self.clock.clone(),
        })
    }

    async fn shared_object_input(&self, object_id: &str) -> Result<ObjectInput, DexError> {
        let shared = self.grpc.get_shared_object(object_id).await?;
        Ok(ObjectInput::shared(
            shared.object_id,
            shared.initial_shared_version,
            shared.mutable,
        ))
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
    exclude: Option<Address>,
) -> Result<&'a CoinRef, DexError> {
    let needed = amount_to_coin_units(amount, coin_type, pool)?;
    coins
        .iter()
        .filter(|c| exclude.map(|id| c.object_id != id).unwrap_or(true))
        .find(|c| c.balance >= needed)
        .ok_or_else(|| {
            DexError::Transaction(
                format!("insufficient `{coin_type}` balance for deposit of {amount}").into(),
            )
        })
}
