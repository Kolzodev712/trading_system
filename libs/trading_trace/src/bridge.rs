//! Trace bridge for linking hot and cold path traces.

use crate::CorrelationId;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

type CorrelationMapEntry = (Option<u64>, Option<u64>);

#[derive(Debug, Clone)]
pub struct TraceBridge {
    hot_to_cold: Arc<RwLock<HashMap<u64, u64>>>,
    correlation_map: Arc<RwLock<HashMap<CorrelationId, CorrelationMapEntry>>>,
}

impl TraceBridge {
    pub fn new() -> Self {
        Self {
            hot_to_cold: Arc::new(RwLock::new(HashMap::new())),
            correlation_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn link(&self, hot_trace_id: u64, cold_trace_id: u64) {
        self.hot_to_cold.write().insert(hot_trace_id, cold_trace_id);
    }

    pub fn register_correlation(
        &self,
        correlation_id: CorrelationId,
        hot_trace_id: Option<u64>,
        cold_trace_id: Option<u64>,
    ) {
        self.correlation_map
            .write()
            .insert(correlation_id, (hot_trace_id, cold_trace_id));
    }

    pub fn get_cold_trace(&self, hot_trace_id: u64) -> Option<u64> {
        self.hot_to_cold.read().get(&hot_trace_id).copied()
    }

    pub fn get_by_correlation(&self, correlation_id: CorrelationId) -> Option<CorrelationMapEntry> {
        self.correlation_map.read().get(&correlation_id).copied()
    }

    pub fn cleanup_old_correlations(&self, threshold: CorrelationId) {
        self.correlation_map
            .write()
            .retain(|&id, _| id >= threshold);
    }

    pub fn stats(&self) -> TraceBridgeStats {
        let hot_to_cold = self.hot_to_cold.read();
        let correlation_map = self.correlation_map.read();
        TraceBridgeStats {
            hot_to_cold_links: hot_to_cold.len(),
            correlation_entries: correlation_map.len(),
        }
    }
}

impl Default for TraceBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TraceBridgeStats {
    pub hot_to_cold_links: usize,
    pub correlation_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_bridge_new() {
        let bridge = TraceBridge::new();
        let stats = bridge.stats();
        assert_eq!(stats.hot_to_cold_links, 0);
        assert_eq!(stats.correlation_entries, 0);
    }

    #[test]
    fn test_trace_bridge_link() {
        let bridge = TraceBridge::new();
        bridge.link(100, 200);
        assert_eq!(bridge.get_cold_trace(100), Some(200));
        assert_eq!(bridge.get_cold_trace(999), None);
    }

    #[test]
    fn test_trace_bridge_register_correlation() {
        let bridge = TraceBridge::new();
        bridge.register_correlation(12345, Some(100), Some(200));
        let result = bridge.get_by_correlation(12345);
        assert_eq!(result, Some((Some(100), Some(200))));
    }

    #[test]
    fn test_trace_bridge_cleanup() {
        let bridge = TraceBridge::new();
        bridge.register_correlation(100, Some(1), Some(2));
        bridge.register_correlation(200, Some(3), Some(4));
        bridge.register_correlation(300, Some(5), Some(6));

        bridge.cleanup_old_correlations(250);

        assert_eq!(bridge.get_by_correlation(100), None);
        assert_eq!(bridge.get_by_correlation(200), None);
        assert_eq!(bridge.get_by_correlation(300), Some((Some(5), Some(6))));
    }
}
