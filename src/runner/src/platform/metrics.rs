//! Additional Prometheus text for platform metrics (appended to OTEL scrape).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::event_bridge::{BROADCAST_LAG_CEX, BROADCAST_LAG_DEX, BROADCAST_LAG_MD};
use super::zenoh_metrics::{
    ZENOH_TRANSPORT_DROPPED, ZENOH_TRANSPORT_PUBLISH_ERRORS, ZENOH_TRANSPORT_SENT,
};
use dex_engine::runtime::{DEX_EXECUTE_ERRORS_TOTAL, DEX_EXECUTE_TOTAL};
use trading_core::venue::VenueMode;
use trading_trace::ExporterHandles;

/// Build Prometheus scrape bytes (OTEL registry + platform counters).
pub fn build_metrics_provider(
    tracing_handles: Option<ExporterHandles>,
    venue_mode: VenueMode,
) -> Arc<dyn Fn() -> Vec<u8> + Send + Sync> {
    Arc::new(move || {
        let mut out = tracing_handles
            .as_ref()
            .map(trading_trace::encode_prometheus_metrics)
            .transpose()
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_bytes();
        out.extend_from_slice(encode_platform_metrics(venue_mode).as_bytes());
        out
    })
}

pub fn encode_platform_metrics(venue_mode: VenueMode) -> String {
    let mode_value = match venue_mode {
        VenueMode::Cex => 0,
        VenueMode::Dex => 1,
        VenueMode::Both => 2,
    };
    format!(
        "# HELP venue_mode Active venue mode (0=cex, 1=dex, 2=both)\n\
         # TYPE venue_mode gauge\n\
         venue_mode {}\n\
         # HELP dex_execute_total DEX execute calls\n\
         # TYPE dex_execute_total counter\n\
         dex_execute_total {}\n\
         # HELP dex_execute_errors_total DEX execute errors\n\
         # TYPE dex_execute_errors_total counter\n\
         dex_execute_errors_total {}\n\
         # HELP broadcast_lag_total Broadcast receiver lagged messages\n\
         # TYPE broadcast_lag_total counter\n\
         broadcast_lag_total{{channel=\"md\"}} {}\n\
         broadcast_lag_total{{channel=\"cex\"}} {}\n\
         broadcast_lag_total{{channel=\"dex\"}} {}\n\
         # HELP zenoh_transport_sent_total Events published on Zenoh transport\n\
         # TYPE zenoh_transport_sent_total counter\n\
         zenoh_transport_sent_total {}\n\
         # HELP zenoh_transport_dropped_total Events dropped (transport queue full)\n\
         # TYPE zenoh_transport_dropped_total counter\n\
         zenoh_transport_dropped_total {}\n\
         # HELP zenoh_transport_publish_errors_total Zenoh publish failures\n\
         # TYPE zenoh_transport_publish_errors_total counter\n\
         zenoh_transport_publish_errors_total {}\n",
        mode_value,
        DEX_EXECUTE_TOTAL.load(Ordering::Relaxed),
        DEX_EXECUTE_ERRORS_TOTAL.load(Ordering::Relaxed),
        BROADCAST_LAG_MD.load(Ordering::Relaxed),
        BROADCAST_LAG_CEX.load(Ordering::Relaxed),
        BROADCAST_LAG_DEX.load(Ordering::Relaxed),
        ZENOH_TRANSPORT_SENT.load(Ordering::Relaxed),
        ZENOH_TRANSPORT_DROPPED.load(Ordering::Relaxed),
        ZENOH_TRANSPORT_PUBLISH_ERRORS.load(Ordering::Relaxed),
    )
}
