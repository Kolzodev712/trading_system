use crate::events::{ColdEvent, ColdEventRx, DexColdEvent, DexColdEventRx, HotEvent, HotEventRx};
use crate::gateway::backpressure::BackpressurePolicy;
use crate::gateway::transports::health::{SharedTransportHealth, TransportHealth};
use crate::gateway::transports::EventTransport;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub enum Lane {
    Hot,
    Cold,
    DexCold,
}

#[derive(Debug)]
pub struct EventGateway {
    hot_policy: BackpressurePolicy,
    cold_policy: BackpressurePolicy,
    dex_cold_policy: BackpressurePolicy,
    transports: Vec<TransportSlot>,
}

#[derive(Debug)]
struct TransportSlot {
    name: String,
    tx: mpsc::Sender<EventMsg>,
    health: SharedTransportHealth,
}

#[derive(Debug)]
enum EventMsg {
    Hot(HotEvent),
    Cold(Arc<ColdEvent>),
    DexCold(Arc<DexColdEvent>),
}

impl EventGateway {
    pub fn new(hot_policy: BackpressurePolicy, cold_policy: BackpressurePolicy) -> Self {
        Self {
            hot_policy,
            cold_policy,
            dex_cold_policy: cold_policy,
            transports: Vec::new(),
        }
    }

    pub fn with_dex_cold_policy(mut self, policy: BackpressurePolicy) -> Self {
        self.dex_cold_policy = policy;
        self
    }

    pub fn add_transport(
        &mut self,
        name: impl Into<String>,
        transport: Arc<dyn EventTransport>,
        queue_capacity: usize,
    ) -> SharedTransportHealth {
        let (tx, mut rx) = mpsc::channel::<EventMsg>(queue_capacity);
        let health = Arc::new(TransportHealth::new(queue_capacity));
        let health_for_task = Arc::clone(&health);
        let name_str = name.into();

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    EventMsg::Hot(e) => match transport.publish_hot(e).await {
                        Ok(()) => health_for_task.inc_sent(),
                        Err(err) => health_for_task.inc_publish_error(err),
                    },
                    EventMsg::Cold(e) => match transport.publish_cold(e).await {
                        Ok(()) => health_for_task.inc_sent(),
                        Err(err) => health_for_task.inc_publish_error(err),
                    },
                    EventMsg::DexCold(e) => match transport.publish_dex_cold(e).await {
                        Ok(()) => health_for_task.inc_sent(),
                        Err(err) => health_for_task.inc_publish_error(err),
                    },
                }
            }
        });

        self.transports.push(TransportSlot {
            name: name_str,
            tx,
            health: Arc::clone(&health),
        });

        health
    }

    pub fn transport_health(&self) -> impl Iterator<Item = (&str, SharedTransportHealth)> {
        self.transports
            .iter()
            .map(|t| (t.name.as_str(), Arc::clone(&t.health)))
    }

    pub async fn run(
        mut self,
        mut hot_rx: HotEventRx,
        mut cold_rx: ColdEventRx,
        mut dex_cold_rx: DexColdEventRx,
    ) {
        let mut hot_counter: u64 = 0;
        let mut cold_counter: u64 = 0;
        let mut dex_cold_counter: u64 = 0;

        loop {
            tokio::select! {
                hot = hot_rx.recv() => {
                    let Some(event) = hot else { break; };
                    self.fanout(Lane::Hot, EventMsg::Hot(event), &mut hot_counter);
                }
                cold = cold_rx.recv() => {
                    match cold {
                        Ok(event) => self.fanout(Lane::Cold, EventMsg::Cold(event), &mut cold_counter),
                        Err(crate::events::ColdEventBusError::Lagged(_)) => continue,
                        Err(crate::events::ColdEventBusError::Closed) => break,
                        Err(crate::events::ColdEventBusError::NoReceivers) => continue,
                    }
                }
                dex_cold = dex_cold_rx.recv() => {
                    match dex_cold {
                        Ok(event) => self.fanout(Lane::DexCold, EventMsg::DexCold(event), &mut dex_cold_counter),
                        Err(crate::events::DexColdEventBusError::Lagged(_)) => continue,
                        Err(crate::events::DexColdEventBusError::Closed) => break,
                        Err(crate::events::DexColdEventBusError::NoReceivers) => continue,
                    }
                }
            }
        }
    }

    fn fanout(&mut self, lane: Lane, msg: EventMsg, counter: &mut u64) {
        let policy = match lane {
            Lane::Hot => self.hot_policy,
            Lane::Cold => self.cold_policy,
            Lane::DexCold => self.dex_cold_policy,
        };
        if !policy.should_forward(counter) {
            return;
        }

        for t in &self.transports {
            let r = t.tx.try_send(match &msg {
                EventMsg::Hot(e) => EventMsg::Hot(e.clone()),
                EventMsg::Cold(e) => EventMsg::Cold(Arc::clone(e)),
                EventMsg::DexCold(e) => EventMsg::DexCold(Arc::clone(e)),
            });
            if r.is_err() {
                t.health.inc_dropped();
            }
        }
    }
}
