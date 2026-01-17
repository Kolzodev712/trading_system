//! WebSocket subscription management

use smol_str::SmolStr;

use super::types::ws_streaming::{BybitPingRequest, BybitSubscribeRequest, BybitWebSocketOp};

pub const TOPIC_POSITION: &str = "position";
pub const TOPIC_EXECUTION: &str = "execution";
pub const TOPIC_ORDER: &str = "order";
pub const TOPIC_WALLET: &str = "wallet";

pub struct BybitSubscriber;

impl BybitSubscriber {
    pub fn create_subscribe_request(
        topics: Vec<&str>,
        req_id: Option<SmolStr>,
    ) -> BybitSubscribeRequest {
        BybitSubscribeRequest {
            req_id,
            op: BybitWebSocketOp::Subscribe,
            args: topics.into_iter().map(SmolStr::from).collect(),
        }
    }

    pub fn create_unsubscribe_request(
        topics: Vec<&str>,
        req_id: Option<SmolStr>,
    ) -> BybitSubscribeRequest {
        BybitSubscribeRequest {
            req_id,
            op: BybitWebSocketOp::Unsubscribe,
            args: topics.into_iter().map(SmolStr::from).collect(),
        }
    }

    pub fn create_ping_request(req_id: Option<SmolStr>) -> BybitPingRequest {
        BybitPingRequest {
            req_id,
            op: BybitWebSocketOp::Ping,
        }
    }

    pub fn default_topics() -> Vec<&'static str> {
        vec![TOPIC_POSITION, TOPIC_EXECUTION, TOPIC_ORDER, TOPIC_WALLET]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_subscribe_request() {
        let topics = vec!["position", "execution"];
        let req = BybitSubscriber::create_subscribe_request(topics, Some("req-123".into()));

        assert_eq!(req.req_id, Some("req-123".into()));
        assert_eq!(req.op, BybitWebSocketOp::Subscribe);
        assert_eq!(req.args.len(), 2);
        assert_eq!(req.args[0], "position");
        assert_eq!(req.args[1], "execution");
    }

    #[test]
    fn test_create_unsubscribe_request() {
        let topics = vec!["position"];
        let req = BybitSubscriber::create_unsubscribe_request(topics, Some("req-456".into()));

        assert_eq!(req.req_id, Some("req-456".into()));
        assert_eq!(req.op, BybitWebSocketOp::Unsubscribe);
        assert_eq!(req.args.len(), 1);
        assert_eq!(req.args[0], "position");
    }

    #[test]
    fn test_create_ping_request() {
        let req = BybitSubscriber::create_ping_request(Some("ping-123".into()));

        assert_eq!(req.req_id, Some("ping-123".into()));
        assert_eq!(req.op, BybitWebSocketOp::Ping);
    }

    #[test]
    fn test_create_ping_request_no_req_id() {
        let req = BybitSubscriber::create_ping_request(None);

        assert_eq!(req.req_id, None);
        assert_eq!(req.op, BybitWebSocketOp::Ping);
    }

    #[test]
    fn test_default_topics() {
        let topics = BybitSubscriber::default_topics();

        assert_eq!(topics.len(), 4);
        assert_eq!(topics[0], TOPIC_POSITION);
        assert_eq!(topics[1], TOPIC_EXECUTION);
        assert_eq!(topics[2], TOPIC_ORDER);
        assert_eq!(topics[3], TOPIC_WALLET);
    }
}
