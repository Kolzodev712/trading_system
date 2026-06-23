//! Optional Zenoh event/command gateway (opt-in via `ZENOH_ENABLED`).

use std::sync::Arc;

use tracing::warn;
use trading_networking::commands::MpscCommandRouter;
use trading_networking::gateway::backpressure::BackpressurePolicy;
use trading_networking::gateway::command_gateway::serve_zenoh_commands;
use trading_networking::gateway::event_gateway::EventGateway;
use trading_networking::gateway::transports::zenoh::{
    open_default_session, ZenohEventTransport, ZenohKeyspace,
};

use super::zenoh_metrics;

pub(crate) async fn maybe_start_zenoh_gateway(
    hot_rx: trading_networking::events::HotEventRx,
    cold_bus: trading_networking::events::ColdEventBus,
    dex_bus: trading_networking::events::DexColdEventBus,
    router: Arc<MpscCommandRouter>,
) {
    let enabled = std::env::var("ZENOH_ENABLED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !enabled {
        return;
    }

    let session = match open_default_session().await {
        Ok(s) => s,
        Err(e) => {
            warn!("Zenoh session open failed: {e}");
            return;
        }
    };
    let keyspace = ZenohKeyspace::default();
    let transport = Arc::new(ZenohEventTransport::new(session.clone(), keyspace.clone()));

    let mut gateway = EventGateway::new(
        BackpressurePolicy::Sample { every_nth: 1 },
        BackpressurePolicy::Drop,
    );
    let zenoh_health = gateway.add_transport("zenoh", transport, 1024);

    let cold_rx = cold_bus.subscribe();
    let dex_rx = dex_bus.subscribe();
    tokio::spawn(async move {
        gateway.run(hot_rx, cold_rx, dex_rx).await;
    });

    tokio::spawn(async move {
        loop {
            let m = zenoh_health.snapshot();
            zenoh_metrics::record_transport_snapshot(m.sent, m.dropped, m.publish_errors);
            if m.publish_errors > 0 {
                if let Some(ref err) = m.last_error {
                    warn!(
                        zenoh_transport = "zenoh",
                        sent = m.sent,
                        dropped = m.dropped,
                        publish_errors = m.publish_errors,
                        last_error = %err,
                        "Zenoh transport health"
                    );
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });

    let cmd_key = keyspace.commands_key;
    tokio::spawn(async move {
        if let Err(e) = serve_zenoh_commands(&session, &cmd_key, router).await {
            tracing::warn!("Zenoh command gateway stopped: {e}");
        }
    });
}
