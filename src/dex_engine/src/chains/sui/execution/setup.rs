//! One-off DeepBook setup helpers (BalanceManager creation, etc.).

use sui_sdk_types::Address;
use sui_transaction_builder::TransactionBuilder;
use tracing::info;
use trading_core::venue::dex::DexError;

use super::super::config::DeepBookConfig;
use super::super::grpc::SuiGrpcClient;
use super::balance_manager::append_create_register_and_share;
use super::keys::SuiSigner;
use super::ptb::parse_address;
use super::rpc_types::{find_created_in_changes, ExecuteTxResult};
use super::submit::finish_and_submit;

/// Offline-capable setup client (create BalanceManager, etc.).
pub struct DeepBookSetup {
    config: DeepBookConfig,
    signer: SuiSigner,
    grpc: SuiGrpcClient,
}

impl DeepBookSetup {
    pub async fn from_config(config: DeepBookConfig) -> Result<Self, DexError> {
        let signer = SuiSigner::from_env(config.private_key_env.as_str())?;
        let grpc = SuiGrpcClient::connect(config.grpc_url())?;
        Ok(Self {
            config,
            signer,
            grpc,
        })
    }

    pub fn signer_address(&self) -> Address {
        self.signer.address()
    }

    pub fn config(&self) -> &DeepBookConfig {
        &self.config
    }

    /// Create, register, and share a new BalanceManager. Returns the on-chain object ID.
    pub async fn create_and_register_balance_manager(&self) -> Result<Address, DexError> {
        let package_id = parse_address(&self.config.deepbook_package_id()?)?;
        let registry = self
            .grpc
            .get_shared_object(self.config.registry_id().as_str())
            .await?;

        let mut builder = TransactionBuilder::new();
        append_create_register_and_share(&mut builder, package_id, &registry)?;

        let exec = self.finish_and_submit(builder).await?;
        info!("BalanceManager setup tx digest={}", exec.digest);

        if exec.status == trading_core::venue::dex::types::TxStatus::Failed {
            return Err(DexError::Transaction(
                format!(
                    "BalanceManager setup transaction `{}` failed on-chain",
                    exec.digest
                )
                .into(),
            ));
        }

        let manager_id = find_created_in_changes(
            exec.changed_objects.as_slice(),
            "balance_manager::BalanceManager",
        )
        .or(self
            .grpc
            .find_created_object(&exec.digest, "balance_manager::BalanceManager")
            .await
            .ok())
        .ok_or_else(|| {
            DexError::Rpc(
                format!(
                    "BalanceManager setup digest={} but no BalanceManager in changed_objects",
                    exec.digest
                )
                .into(),
            )
        })?;

        Ok(manager_id)
    }

    async fn finish_and_submit(
        &self,
        builder: TransactionBuilder,
    ) -> Result<ExecuteTxResult, DexError> {
        finish_and_submit(&self.grpc, &self.signer, builder).await
    }
}
