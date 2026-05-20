use tracing::warn;
use trading_core::venue::dex::channels::{DexVenueUpdate, DexVenueUpdateSender};
use trading_core::venue::dex::DexError;
use trading_trace::{get_tracer, ColdTraceContext};

#[derive(Debug, Clone)]
pub struct DexStatePublisher {
    tx: DexVenueUpdateSender,
}

impl DexStatePublisher {
    pub fn new(tx: DexVenueUpdateSender) -> Self {
        Self { tx }
    }

    pub fn publish(&self, update: DexVenueUpdate) -> Result<(), DexError> {
        let update = match update {
            DexVenueUpdate::WalletBalance { balances, .. } => {
                let tracer = get_tracer("dex_engine");
                let ctx =
                    ColdTraceContext::new_root_with_tracer(&tracer, "dex.wallet_balance", None);
                let to_send = ctx.clone();
                ctx.end();
                DexVenueUpdate::WalletBalance {
                    balances,
                    trace_context: Some(to_send),
                }
            }
            DexVenueUpdate::PendingTransaction { pending, .. } => {
                let tracer = get_tracer("dex_engine");
                let ctx = ColdTraceContext::new_root_with_tracer(
                    &tracer,
                    "dex.pending_transaction",
                    None,
                );
                let to_send = ctx.clone();
                ctx.end();
                DexVenueUpdate::PendingTransaction {
                    pending,
                    trace_context: Some(to_send),
                }
            }
            DexVenueUpdate::ExecutionResult { result, .. } => {
                let tracer = get_tracer("dex_engine");
                let ctx =
                    ColdTraceContext::new_root_with_tracer(&tracer, "dex.execution_result", None);
                let to_send = ctx.clone();
                ctx.end();
                DexVenueUpdate::ExecutionResult {
                    result,
                    trace_context: Some(to_send),
                }
            }
        };

        self.tx.send(update).map(|_| ()).map_err(|e| {
            warn!("DEX venue update channel send failed: {e}");
            DexError::ChannelSend(e.to_string().into())
        })
    }
}
