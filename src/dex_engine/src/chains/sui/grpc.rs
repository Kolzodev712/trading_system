//! Sui full-node gRPC client via Mysten's `sui-rpc` crate.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use prost_types::FieldMask;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    owner, Bcs, ExecuteTransactionRequest, GetCoinInfoRequest, GetObjectRequest,
    GetTransactionRequest, ListBalancesRequest, ListOwnedObjectsRequest, Transaction,
    UserSignature,
};
use sui_rpc::Client;
use sui_sdk_types::{Address, StructTag, TypeTag};
use tracing::debug;
use trading_core::venue::dex::types::TxStatus;
use trading_core::venue::dex::DexError;

use crate::chains::sui::execution::rpc_types::{
    map_execution_status, rpc_err, CoinRef, ExecuteTxResult, SharedObjectRef, SUI_COIN_TYPE,
};

const TX_READ_MASK: &str = "digest,effects.status,effects.changed_objects";
const COIN_READ_MASK: [&str; 5] = ["object_id", "version", "digest", "balance", "object_type"];

#[derive(Clone)]
pub struct SuiGrpcClient {
    endpoint: SmolStr,
    client: Client,
    decimals_cache: Arc<RwLock<HashMap<SmolStr, u32>>>,
}

impl SuiGrpcClient {
    pub fn connect(grpc_url: impl Into<SmolStr>) -> Result<Self, DexError> {
        let endpoint = grpc_url.into();
        let url = normalize_grpc_url(endpoint.as_str());
        let client = Client::new(url).map_err(|e| rpc_err(format!("gRPC connect: {e}")))?;
        Ok(Self {
            endpoint,
            client,
            decimals_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn list_balances(
        &self,
        owner: &str,
    ) -> Result<trading_core::venue::dex::types::DexWalletBalances, DexError> {
        let mut page_token = None;
        let mut entries = Vec::new();
        loop {
            let mut request = ListBalancesRequest::default()
                .with_owner(owner)
                .with_page_size(100);
            if let Some(token) = page_token {
                request = request.with_page_token(token);
            }
            debug!(
                "Sui gRPC StateService.ListBalances -> {} owner={owner}",
                self.endpoint
            );
            let mut client = self.client.clone();
            let response = client
                .state_client()
                .list_balances(request)
                .await
                .map_err(|e| rpc_err(format!("ListBalances: {e}")))?;
            let body = response.into_inner();
            entries.extend(body.balances);
            match body.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }
        self.balances_to_wallet(&entries).await
    }

    pub async fn coin_decimals(&self, coin_type: &str) -> Result<u32, DexError> {
        let key = SmolStr::new(coin_type);
        if let Some(decimals) = self.decimals_cache.read().get(&key).copied() {
            return Ok(decimals);
        }
        let request = GetCoinInfoRequest::default().with_coin_type(coin_type);
        debug!("Sui gRPC StateService.GetCoinInfo coin_type={coin_type}");
        let mut client = self.client.clone();
        let response = client
            .state_client()
            .get_coin_info(request)
            .await
            .map_err(|e| rpc_err(format!("GetCoinInfo: {e}")))?;
        let decimals = response
            .into_inner()
            .metadata_opt()
            .and_then(|m| m.decimals_opt())
            .unwrap_or_else(|| decimals_for_coin_type(coin_type));
        self.decimals_cache.write().insert(key, decimals);
        Ok(decimals)
    }

    pub async fn get_reference_gas_price(&self) -> Result<u64, DexError> {
        debug!("Sui gRPC LedgerService.GetEpoch -> {}", self.endpoint);
        let mut client = self.client.clone();
        client
            .get_reference_gas_price()
            .await
            .map_err(|e| rpc_err(format!("GetEpoch: {e}")))
    }

    pub async fn get_sui_coins(&self, owner: &str) -> Result<Vec<CoinRef>, DexError> {
        self.get_coins(owner, SUI_COIN_TYPE).await
    }

    pub async fn get_coins(&self, owner: &str, coin_type: &str) -> Result<Vec<CoinRef>, DexError> {
        let type_tag = parse_coin_type_tag(coin_type)?;
        let struct_tag = StructTag::coin(type_tag);
        let mut page_token = None;
        let mut coins = Vec::new();
        loop {
            let mut request = ListOwnedObjectsRequest::default()
                .with_owner(owner)
                .with_page_size(100)
                .with_object_type(struct_tag.to_string())
                .with_read_mask(FieldMask::from_paths(COIN_READ_MASK));
            if let Some(token) = page_token {
                request = request.with_page_token(token);
            }
            debug!("Sui gRPC StateService.ListOwnedObjects owner={owner} type={struct_tag}");
            let mut client = self.client.clone();
            let response = client
                .state_client()
                .list_owned_objects(request)
                .await
                .map_err(|e| rpc_err(format!("ListOwnedObjects: {e}")))?;
            let body = response.into_inner();
            for obj in body.objects {
                let object_id = obj.object_id();
                let version = obj.version();
                let digest = obj.digest();
                let balance = obj.balance();
                if balance == 0 {
                    continue;
                }
                coins.push(CoinRef {
                    object_id: object_id.parse().map_err(|e| {
                        rpc_err(format!("invalid coin object id `{object_id}`: {e}"))
                    })?,
                    version,
                    digest: digest
                        .parse()
                        .map_err(|e| rpc_err(format!("invalid coin digest `{digest}`: {e}")))?,
                    balance,
                });
            }
            match body.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }
        Ok(coins)
    }

    pub async fn get_shared_object(&self, object_id: &str) -> Result<SharedObjectRef, DexError> {
        let parsed_id: Address = object_id
            .parse()
            .map_err(|e| rpc_err(format!("invalid object id `{object_id}`: {e}")))?;
        let request = GetObjectRequest::new(&parsed_id)
            .with_read_mask(FieldMask::from_paths(["object_id", "owner"]));
        debug!("Sui gRPC LedgerService.GetObject object_id={object_id}");
        let mut client = self.client.clone();
        let response = client
            .ledger_client()
            .get_object(request)
            .await
            .map_err(|e| rpc_err(format!("GetObject: {e}")))?;
        let body = response.into_inner();
        let obj = body
            .object_opt()
            .cloned()
            .ok_or_else(|| rpc_err(format!("object `{object_id}` not found")))?;
        let owner = obj.owner();
        if owner.kind != Some(owner::OwnerKind::Shared as i32) {
            return Err(rpc_err(format!(
                "object `{object_id}` is not shared (owner_kind={:?})",
                owner.kind
            )));
        }
        Ok(SharedObjectRef {
            object_id: parsed_id,
            initial_shared_version: owner.version_opt().unwrap_or(0),
            mutable: true,
        })
    }

    pub(crate) async fn execute_transaction_block(
        &self,
        tx_bytes: &[u8],
        signatures: &[&[u8]],
    ) -> Result<ExecuteTxResult, DexError> {
        let transaction = Transaction::default().with_bcs(Bcs::from(tx_bytes.to_vec()));
        let sigs: Vec<UserSignature> = signatures
            .iter()
            .map(|s| UserSignature::default().with_bcs(Bcs::from(s.to_vec())))
            .collect();
        let request = ExecuteTransactionRequest::new(transaction)
            .with_signatures(sigs)
            .with_read_mask(FieldMask::from_str(TX_READ_MASK));
        debug!("Sui gRPC TransactionExecutionService.ExecuteTransaction");
        let mut client = self.client.clone();
        let response = client
            .execution_client()
            .execute_transaction(request)
            .await
            .map_err(|e| rpc_err(format!("ExecuteTransaction: {e}")))?;
        let body = response.into_inner();
        let executed = body
            .transaction_opt()
            .cloned()
            .ok_or_else(|| rpc_err("ExecuteTransaction: empty response"))?;
        let digest = SmolStr::new(executed.digest());
        let status = executed
            .effects_opt()
            .and_then(|e| e.status_opt())
            .map(map_execution_status)
            .unwrap_or(TxStatus::Pending);
        let changed_objects = executed
            .effects_opt()
            .map(|e| e.changed_objects().to_vec())
            .unwrap_or_default();
        Ok(ExecuteTxResult {
            digest,
            status,
            changed_objects,
        })
    }

    pub async fn get_transaction_status(&self, digest: &str) -> Result<TxStatus, DexError> {
        let executed = self.get_transaction(digest).await?;
        let effects = executed
            .effects_opt()
            .ok_or_else(|| rpc_err(format!("transaction `{digest}` missing effects")))?;
        let status = effects
            .status_opt()
            .ok_or_else(|| rpc_err(format!("transaction `{digest}` missing status")))?;
        Ok(map_execution_status(status))
    }

    pub async fn find_created_object(
        &self,
        digest: &str,
        type_contains: &str,
    ) -> Result<Address, DexError> {
        let executed = self.get_transaction_with_retry(digest, 5).await?;
        let changes = executed
            .effects_opt()
            .map(|e| e.changed_objects())
            .unwrap_or(&[]);
        crate::chains::sui::execution::rpc_types::find_created_in_changes(changes, type_contains)
            .ok_or_else(|| {
                rpc_err(format!(
                    "no created object matching `{type_contains}` in transaction `{digest}`"
                ))
            })
    }

    async fn get_transaction(
        &self,
        digest: &str,
    ) -> Result<sui_rpc::proto::sui::rpc::v2::ExecutedTransaction, DexError> {
        let request = GetTransactionRequest::default()
            .with_digest(digest)
            .with_read_mask(FieldMask::from_str(TX_READ_MASK));
        let mut client = self.client.clone();
        let response = client
            .ledger_client()
            .get_transaction(request)
            .await
            .map_err(|e| rpc_err(format!("GetTransaction: {e}")))?;
        let body = response.into_inner();
        body.transaction_opt()
            .cloned()
            .ok_or_else(|| rpc_err(format!("transaction `{digest}` not found")))
    }

    async fn get_transaction_with_retry(
        &self,
        digest: &str,
        attempts: u32,
    ) -> Result<sui_rpc::proto::sui::rpc::v2::ExecutedTransaction, DexError> {
        let mut last_err = None;
        for attempt in 0..attempts {
            match self.get_transaction(digest).await {
                Ok(r) => return Ok(r),
                Err(e) => {
                    let retry = matches!(&e, DexError::Rpc(msg) if msg.contains("not found"));
                    if !retry || attempt + 1 == attempts {
                        return Err(e);
                    }
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_millis(400 * (attempt as u64 + 1))).await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| rpc_err("GetTransaction exhausted retries")))
    }

    async fn balances_to_wallet(
        &self,
        entries: &[sui_rpc::proto::sui::rpc::v2::Balance],
    ) -> Result<trading_core::venue::dex::types::DexWalletBalances, DexError> {
        use trading_core::venue::dex::types::DexWalletBalances;

        let mut balances = DexWalletBalances::default();
        for entry in entries {
            let coin_type = entry.coin_type();
            if coin_type.is_empty() {
                continue;
            }
            let raw = entry.balance();
            if raw == 0 {
                continue;
            }
            let symbol = coin_type_to_symbol(coin_type);
            let decimals = self.coin_decimals(coin_type).await?;
            let amount = Decimal::from(raw) / Decimal::from(10u64.pow(decimals));
            balances.balances.insert(symbol, amount);
        }
        Ok(balances)
    }
}

fn parse_coin_type_tag(coin_type: &str) -> Result<TypeTag, DexError> {
    coin_type
        .parse()
        .map_err(|e| rpc_err(format!("invalid coin type `{coin_type}`: {e}")))
}

fn normalize_grpc_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

pub fn coin_type_to_symbol(coin_type: &str) -> SmolStr {
    coin_type.rsplit("::").next().unwrap_or(coin_type).into()
}

pub fn decimals_for_coin_type(coin_type: &str) -> u32 {
    if coin_type.ends_with("::SUI") || coin_type.contains("::sui::SUI") {
        9
    } else {
        6
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;
    use sui_rpc::proto::sui::rpc::v2::Balance;

    #[test]
    fn normalizes_bare_host_to_https() {
        assert_eq!(
            normalize_grpc_url("fullnode.testnet.sui.io"),
            "https://fullnode.testnet.sui.io"
        );
    }

    #[test]
    fn coin_type_to_symbol_takes_last_segment() {
        assert_eq!(coin_type_to_symbol("0x2::sui::SUI").as_str(), "SUI");
    }

    #[test]
    fn heuristic_decimals_for_sui_and_stablecoins() {
        assert_eq!(decimals_for_coin_type("0x2::sui::SUI"), 9);
        assert_eq!(decimals_for_coin_type("0xf715::DBUSDC::DBUSDC"), 6);
    }

    #[tokio::test]
    async fn converts_balances_with_cached_decimals() {
        let client = SuiGrpcClient {
            endpoint: SmolStr::new("https://fullnode.testnet.sui.io"),
            client: Client::new("https://fullnode.testnet.sui.io").unwrap(),
            decimals_cache: Arc::new(RwLock::new(HashMap::from([(
                SmolStr::new("0x2::sui::SUI"),
                9,
            )]))),
        };
        let entries = vec![Balance::default()
            .with_coin_type("0x2::sui::SUI")
            .with_balance(1_500_000_000)];
        let wallet = client.balances_to_wallet(&entries).await.unwrap();
        assert_eq!(
            wallet.balances.get(&SmolStr::new("SUI")),
            Some(&Decimal::from_str("1.5").unwrap())
        );
    }
}
