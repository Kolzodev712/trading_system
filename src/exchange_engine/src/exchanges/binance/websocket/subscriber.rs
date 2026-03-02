//! WebSocket subscription management
//!
//! Handles session logon, RPC request creation, and subscription logic

use super::types::{
    common::{SessionLogonMessage, SessionLogonParams},
    rpc::{
        amend_order::{BinanceWsAmendOrderReqParams, BinanceWsAmendOrderRequest},
        balance::{BinanceAccountStatusParams, BinanceAccountStatusRequest},
        cancel_all::{BinanceWsCancelAllOrdersReqParams, BinanceWsCancelAllOrdersRequest},
        cancel_order::{BinanceWsCancelOrderReqParams, BinanceWsCancelOrderRequest},
        open_order::{BinanceWsOrderReqParams, BinanceWsOrderRequest},
        BinanceRpcRequest,
    },
    user_data::events::BinanceWSMessage,
};
use crate::exchanges::binance::authenticator::BinanceAuthenticator;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::protocol::Message;
use trading_core::{
    authenticator::ExchangeAuthenticator, error::EngineError, utils::generate_custom_uuid,
    websocket::client::SubscriptionHandler,
};

pub const LOGON_SESSION: &str = "session.logon";
pub const ACCOUNT_STATUS: &str = "account.status";
pub const OPEN_ORDER: &str = "order.place";
pub const TEST_ORDER: &str = "order.test";
pub const CANCEL_ORDER: &str = "order.cancel";
pub const CANCEL_ALL_ORDERS: &str = "openOrders.cancelAll";
pub const AMEND_ORDER: &str = "order.modify";

#[derive(Debug, Clone)]
pub struct BinanceSubscriptionHandler {
    signer: BinanceAuthenticator,
    session: Arc<RwLock<Option<SessionLogonParams>>>,
}

impl BinanceSubscriptionHandler {
    pub fn new(
        signer: BinanceAuthenticator,
        session: Arc<RwLock<Option<SessionLogonParams>>>,
    ) -> Self {
        Self { signer, session }
    }

    pub fn session_handle(&self) -> Arc<RwLock<Option<SessionLogonParams>>> {
        Arc::clone(&self.session)
    }

    pub async fn set_session(&self, params: SessionLogonParams) {
        let mut guard = self.session.write().await;
        *guard = Some(params);
    }

    // May be used for other RPC requests that don't need fresh signatures
    pub async fn get_session(&self) -> Result<SessionLogonParams, EngineError> {
        let guard = self.session.read().await;
        guard.clone().ok_or_else(|| {
            EngineError::new_custom_error("No session available for RPC request".into())
        })
    }

    fn build_session_logon_params(&self) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let mut params = BTreeMap::new();
        params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        params.insert("timestamp".into(), timestamp.to_string().into());

        let signed_params = self.signer.build_signed_params(params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    pub fn create_logon_message(&self) -> Result<(Message, SessionLogonParams), EngineError> {
        let logon_params = self.build_session_logon_params()?;

        let logon_msg = SessionLogonMessage {
            id: generate_custom_uuid().into(),
            method: LOGON_SESSION.into(),
            params: logon_params.clone(),
        };

        Ok((
            Message::Text(serde_json::to_string(&logon_msg)?),
            logon_params,
        ))
    }

    fn build_signed_account_status_params(
        &self,
        recv_window: Option<u64>,
        omit_zero_balances: Option<bool>,
    ) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        // Include ALL parameters that will be sent in the signature calculation
        let mut signed_params = BTreeMap::new();
        signed_params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        signed_params.insert("timestamp".into(), timestamp.to_string().into());

        // Include recvWindow and omitZeroBalances in signature if they're present
        if let Some(recv_window) = recv_window {
            signed_params.insert("recvWindow".into(), recv_window.to_string().into());
        }
        if let Some(omit_zero) = omit_zero_balances {
            signed_params.insert("omitZeroBalances".into(), omit_zero.to_string().into());
        }

        let signed_params = self.signer.build_signed_params(signed_params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    pub async fn create_account_status_request(&self) -> Result<BinanceRpcRequest, EngineError> {
        // Generate a NEW signature with a NEW timestamp for each request
        // Reusing the logon session would cause signature validation to fail
        // Include recvWindow and omitZeroBalances in the signature calculation
        let recv_window = Some(60_000);
        let omit_zero_balances = Some(true);

        let session = self.build_signed_account_status_params(recv_window, omit_zero_balances)?;

        let request = BinanceAccountStatusRequest {
            id: generate_custom_uuid().into(),
            method: ACCOUNT_STATUS.into(),
            params: BinanceAccountStatusParams {
                session_logon_params: session,
                recv_window,
                omit_zero_balances,
            },
        };

        Ok(BinanceRpcRequest::AccountStatus(request))
    }

    fn build_signed_order_params(
        &self,
        params: &BinanceWsOrderReqParams,
    ) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let mut signed_params = BTreeMap::new();
        signed_params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        signed_params.insert("timestamp".into(), timestamp.to_string().into());
        signed_params.insert("symbol".into(), params.symbol.clone());
        signed_params.insert("side".into(), params.side.to_string().into());
        signed_params.insert("type".into(), params.order_type.to_string().into());
        if let Some(tif) = &params.time_in_force {
            signed_params.insert("timeInForce".into(), tif.to_string().into());
        }
        if let Some(price) = &params.price {
            signed_params.insert("price".into(), price.to_string().into());
        }
        if let Some(quantity) = &params.quantity {
            signed_params.insert("quantity".into(), quantity.to_string().into());
        }
        if let Some(quote_qty) = &params.quote_order_qty {
            signed_params.insert("quoteOrderQty".into(), quote_qty.to_string().into());
        }

        let signed_params = self.signer.build_signed_params(signed_params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    pub async fn create_test_order_request(
        &self,
        mut params: BinanceWsOrderReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.build_signed_order_params(&params)?;
        params.session = session;

        let request = BinanceWsOrderRequest {
            id: generate_custom_uuid().into(),
            method: TEST_ORDER.into(),
            params,
        };

        Ok(BinanceRpcRequest::OpenOrder(request))
    }

    pub async fn create_open_order_request(
        &self,
        mut params: BinanceWsOrderReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.build_signed_order_params(&params)?;
        params.session = session;

        let request = BinanceWsOrderRequest {
            id: generate_custom_uuid().into(),
            method: OPEN_ORDER.into(),
            params,
        };

        Ok(BinanceRpcRequest::OpenOrder(request))
    }

    fn build_signed_cancel_order_params(
        &self,
        params: &BinanceWsCancelOrderReqParams,
    ) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let mut signed_params = BTreeMap::new();
        signed_params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        signed_params.insert("timestamp".into(), timestamp.to_string().into());
        signed_params.insert("symbol".into(), params.symbol.clone());
        if let Some(order_id) = params.order_id {
            signed_params.insert("orderId".into(), order_id.to_string().into());
        }
        if let Some(orig_client_order_id) = &params.orig_client_order_id {
            signed_params.insert("origClientOrderId".into(), orig_client_order_id.clone());
        }
        if let Some(new_client_order_id) = &params.new_client_order_id {
            signed_params.insert("newClientOrderId".into(), new_client_order_id.clone());
        }
        if let Some(cancel_restrictions) = &params.cancel_restrictions {
            signed_params.insert(
                "cancelRestrictions".into(),
                cancel_restrictions.to_string().into(),
            );
        }

        let signed_params = self.signer.build_signed_params(signed_params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    fn build_signed_cancel_all_params(
        &self,
        params: &BinanceWsCancelAllOrdersReqParams,
    ) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let mut signed_params = BTreeMap::new();
        signed_params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        signed_params.insert("timestamp".into(), timestamp.to_string().into());
        signed_params.insert("symbol".into(), params.symbol.clone());

        let signed_params = self.signer.build_signed_params(signed_params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    pub async fn create_cancel_order_request(
        &self,
        mut params: BinanceWsCancelOrderReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.build_signed_cancel_order_params(&params)?;
        params.session = session;

        let request = BinanceWsCancelOrderRequest {
            id: generate_custom_uuid().into(),
            method: CANCEL_ORDER.into(),
            params,
        };

        Ok(BinanceRpcRequest::CancelOrder(request))
    }

    pub async fn create_cancel_all_orders_request(
        &self,
        mut params: BinanceWsCancelAllOrdersReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.build_signed_cancel_all_params(&params)?;
        params.session = session;

        let request = BinanceWsCancelAllOrdersRequest {
            id: generate_custom_uuid().into(),
            method: CANCEL_ALL_ORDERS.into(),
            params,
        };

        Ok(BinanceRpcRequest::CancelAll(request))
    }

    fn build_signed_amend_order_params(
        &self,
        params: &BinanceWsAmendOrderReqParams,
    ) -> Result<SessionLogonParams, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let mut signed_params = BTreeMap::new();
        signed_params.insert(
            "apiKey".into(),
            self.signer.api_key.expose_smol_str().clone(),
        );
        signed_params.insert("timestamp".into(), timestamp.to_string().into());
        signed_params.insert("symbol".into(), params.symbol.clone());
        if let Some(order_id) = params.order_id {
            signed_params.insert("orderId".into(), order_id.to_string().into());
        }
        if let Some(orig_client_order_id) = &params.orig_client_order_id {
            signed_params.insert("origClientOrderId".into(), orig_client_order_id.clone());
        }
        if let Some(price) = &params.price {
            signed_params.insert("price".into(), price.to_string().into());
        }
        if let Some(quantity) = &params.quantity {
            signed_params.insert("quantity".into(), quantity.to_string().into());
        }

        let signed_params = self.signer.build_signed_params(signed_params)?;

        let api_key = signed_params.get("apiKey").cloned().ok_or_else(|| {
            EngineError::new_custom_error("Missing apiKey in signed params".into())
        })?;

        let timestamp = signed_params
            .get("timestamp")
            .ok_or_else(|| EngineError::new_custom_error("Missing timestamp".into()))?
            .parse::<i64>()
            .map_err(|_| EngineError::new_custom_error("Invalid timestamp format".into()))?;

        let signature = signed_params
            .get("signature")
            .cloned()
            .ok_or_else(|| EngineError::new_custom_error("Missing signature".into()))?;

        Ok(SessionLogonParams {
            api_key,
            timestamp,
            signature,
        })
    }

    pub async fn create_amend_order_request(
        &self,
        mut params: BinanceWsAmendOrderReqParams,
    ) -> Result<BinanceRpcRequest, EngineError> {
        let session = self.build_signed_amend_order_params(&params)?;
        params.session = session;

        let request = BinanceWsAmendOrderRequest {
            id: generate_custom_uuid().into(),
            method: AMEND_ORDER.into(),
            params,
        };

        Ok(BinanceRpcRequest::AmendOrder(request))
    }
}

impl SubscriptionHandler for BinanceSubscriptionHandler {
    fn create_subscription_request(&self) -> Option<Message> {
        let subscription_msg = BinanceWSMessage {
            id: generate_custom_uuid().into(),
            method: "userDataStream.subscribe".into(),
        };

        Some(Message::Text(
            serde_json::to_string(&subscription_msg).ok()?,
        ))
    }
}
