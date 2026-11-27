//! Benchmarks for orderbook delta merge and broadcast fan-out (Arc vs deep clone).
//!
//! Run with: `cargo bench -p market_data_engine --bench orderbook_merge`

use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use rust_decimal::Decimal;
use tokio::sync::broadcast;
use trading_core::types::{
    common::Pair,
    market_data::{OrderbookUpdateKind, StructuredOrderbook},
};

use market_data_engine::state::state::MarketDataState;

const DEPTH: i64 = 200;
const RECEIVERS: usize = 8;

fn pair() -> Pair {
    Pair::new("BTC", "USDT")
}

fn snapshot(depth: i64) -> StructuredOrderbook {
    StructuredOrderbook {
        pair: pair(),
        bids: (0..depth)
            .map(|i| (Decimal::new(10_000 - i, 0), Decimal::new(1 + i % 5, 0)))
            .collect(),
        asks: (0..depth)
            .map(|i| (Decimal::new(10_001 + i, 0), Decimal::new(1 + i % 5, 0)))
            .collect(),
        timestamp: 1,
        update_kind: OrderbookUpdateKind::Snapshot,
        sequence: Some(1),
    }
}

/// Delta touching the top of book: one update, one removal, one insert per side.
fn delta(sequence: u64) -> StructuredOrderbook {
    StructuredOrderbook {
        pair: pair(),
        bids: vec![
            (Decimal::new(10_000, 0), Decimal::new(9, 0)),
            (Decimal::new(9_995, 0), Decimal::ZERO),
            (Decimal::new(10_000 - DEPTH - 1, 0), Decimal::new(3, 0)),
        ],
        asks: vec![
            (Decimal::new(10_001, 0), Decimal::new(9, 0)),
            (Decimal::new(10_006, 0), Decimal::ZERO),
            (Decimal::new(10_001 + DEPTH + 1, 0), Decimal::new(3, 0)),
        ],
        timestamp: sequence,
        update_kind: OrderbookUpdateKind::Delta,
        sequence: Some(sequence),
    }
}

fn bench_merge(c: &mut Criterion) {
    let base = {
        let mut state = MarketDataState::new();
        state.update_orderbook(pair(), snapshot(DEPTH));
        state
    };

    c.bench_function("merge_orderbook_delta/depth_200", |b| {
        b.iter_batched(
            || (base.clone(), delta(2)),
            |(mut state, delta)| black_box(state.update_orderbook(pair(), delta)),
            BatchSize::SmallInput,
        )
    });
}

fn bench_fanout(c: &mut Criterion) {
    let book = snapshot(DEPTH);

    c.bench_function("publish_fanout/arc_clone_8_receivers", |b| {
        let (tx, mut rxs) = channel_with_receivers::<Arc<StructuredOrderbook>>();
        let shared = Arc::new(book.clone());
        b.iter(|| {
            tx.send(Arc::clone(&shared)).unwrap();
            drain(&mut rxs);
        })
    });

    c.bench_function("publish_fanout/deep_clone_8_receivers", |b| {
        let (tx, mut rxs) = channel_with_receivers::<StructuredOrderbook>();
        b.iter(|| {
            tx.send(book.clone()).unwrap();
            drain(&mut rxs);
        })
    });
}

fn channel_with_receivers<T: Clone>() -> (broadcast::Sender<T>, Vec<broadcast::Receiver<T>>) {
    let (tx, rx) = broadcast::channel(16);
    let mut rxs = vec![rx];
    rxs.extend((1..RECEIVERS).map(|_| tx.subscribe()));
    (tx, rxs)
}

fn drain<T: Clone>(rxs: &mut [broadcast::Receiver<T>]) {
    for rx in rxs {
        black_box(rx.try_recv().unwrap());
    }
}

criterion_group!(benches, bench_merge, bench_fanout);
criterion_main!(benches);
