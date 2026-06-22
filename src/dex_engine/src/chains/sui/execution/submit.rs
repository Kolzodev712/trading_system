//! Shared gas selection, signing, and gRPC submission for DeepBook PTBs.

use sui_crypto::SuiSigner as _;
use sui_transaction_builder::TransactionBuilder;
use trading_core::venue::dex::DexError;

use super::super::grpc::SuiGrpcClient;
use super::keys::SuiSigner;
use super::ptb::GAS_BUDGET;
use super::rpc_types::{coin_to_object_input, pick_gas_coin, ExecuteTxResult};

pub async fn finish_and_submit(
    grpc: &SuiGrpcClient,
    signer: &SuiSigner,
    builder: TransactionBuilder,
) -> Result<ExecuteTxResult, DexError> {
    let mut builder = builder;
    let sender = signer.address();
    let gas_price = grpc.get_reference_gas_price().await?;
    let gas_coins = grpc.get_sui_coins(&sender.to_string()).await?;
    let gas_coin = pick_gas_coin(&gas_coins)?;
    builder.set_sender(sender);
    builder.set_gas_budget(GAS_BUDGET);
    builder.set_gas_price(gas_price);
    builder.add_gas_objects([coin_to_object_input(gas_coin)]);

    let transaction = builder
        .try_build()
        .map_err(|e| DexError::Transaction(format!("PTB build failed: {e}").into()))?;

    let signature = signer
        .key()
        .sign_transaction(&transaction)
        .map_err(|e| DexError::Transaction(format!("signing failed: {e}").into()))?;
    let tx_bytes = bcs::to_bytes(&transaction)
        .map_err(|e| DexError::Transaction(format!("tx BCS encode failed: {e}").into()))?;
    let sig_bytes = signature.to_bytes();
    grpc.execute_transaction_block(&tx_bytes, &[&sig_bytes])
        .await
}
