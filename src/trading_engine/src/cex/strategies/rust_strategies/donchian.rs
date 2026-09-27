//! Slow Donchian trend. PropHold when flat / leftover; keep a position until the exit channel.
//! `price_history_size` = entry days (default 20). `min_price_history` = exit days (default 10).

use crate::cex::intent::{StrategyContext, StrategyIntent};
use crate::cex::state::Strategy;
use crate::cex::strategies::rust_strategies::prop_hold::FLAT_EPS;
use crate::cex::strategies::strategy_manager::StrategyManager;
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::collections::VecDeque;
use trading_core::{
    error::EngineError,
    types::{
        common::{Instrument, Pair},
        exchange::Order,
        market_data::Orderbook,
    },
};

#[derive(Debug, Clone)]
struct Bar {
    day: i64,
    high: Decimal,
    low: Decimal,
    close: Decimal,
}

pub struct Donchian {
    symbol: Pair,
    position_size: Decimal,
    entry_n: usize,
    exit_n: usize,
    completed: VecDeque<Bar>,
    forming: Option<Bar>,
}

impl Donchian {
    pub fn new(symbol: Pair, position_size: Decimal, entry_n: usize, exit_n: usize) -> Self {
        let entry_n = entry_n.max(2);
        let exit_n = exit_n.clamp(1, entry_n);
        Self {
            symbol,
            position_size,
            entry_n,
            exit_n,
            completed: VecDeque::new(),
            forming: None,
        }
    }

    fn strategy_id(&self) -> String {
        StrategyManager::make_strategy_id("Donchian", &self.symbol)
    }

    fn day_id(ts: u64) -> i64 {
        let ms = if ts < 1_000_000_000_000 {
            ts.saturating_mul(1000)
        } else {
            ts
        };
        (ms / 86_400_000) as i64
    }

    fn mid(book: &Orderbook) -> Option<Decimal> {
        let bid = book.bids.first()?.0;
        let ask = book.asks.first()?.0;
        Some((bid + ask) / Decimal::TWO)
    }

    fn on_tick(&mut self, ts: u64, price: Decimal) {
        let day = Self::day_id(ts);
        match &mut self.forming {
            Some(forming) if forming.day == day => {
                forming.high = forming.high.max(price);
                forming.low = forming.low.min(price);
                forming.close = price;
            }
            Some(_) => {
                if let Some(done) = self.forming.take() {
                    self.completed.push_back(done);
                    let keep = self.entry_n + 2;
                    while self.completed.len() > keep {
                        self.completed.pop_front();
                    }
                }
                self.forming = Some(Bar {
                    day,
                    high: price,
                    low: price,
                    close: price,
                });
            }
            None => {
                self.forming = Some(Bar {
                    day,
                    high: price,
                    low: price,
                    close: price,
                });
            }
        }
    }

    fn channel(bars: &VecDeque<Bar>, n: usize) -> Option<(Decimal, Decimal)> {
        if bars.len() < n {
            return None;
        }
        let slice = bars.iter().rev().take(n);
        let mut high = None;
        let mut low = None;
        for bar in slice {
            high = Some(high.map_or(bar.high, |h: Decimal| h.max(bar.high)));
            low = Some(low.map_or(bar.low, |l: Decimal| l.min(bar.low)));
        }
        Some((high?, low?))
    }

    fn flatten_leftover(&self, ctx: &StrategyContext<'_>, current: Decimal) -> StrategyIntent {
        if current.abs() <= FLAT_EPS {
            return StrategyIntent::hold(self.strategy_id());
        }
        StrategyIntent::target_position(
            self.strategy_id(),
            Instrument::new(ctx.instrument_type.clone(), self.symbol.clone()),
            Decimal::ZERO,
        )
    }
}

#[async_trait]
impl Strategy for Donchian {
    async fn seed_daily_bars(
        &mut self,
        bars: &[(u64, Decimal, Decimal, Decimal)],
    ) -> Result<(), EngineError> {
        self.completed.clear();
        self.forming = None;
        let today = Self::day_id(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        );
        for &(ts, high, low, close) in bars {
            let day = Self::day_id(ts);
            let bar = Bar {
                day,
                high,
                low,
                close,
            };
            if day >= today {
                self.forming = Some(bar);
            } else {
                self.completed.push_back(bar);
            }
        }
        let keep = self.entry_n + 2;
        while self.completed.len() > keep {
            self.completed.pop_front();
        }
        Ok(())
    }

    async fn evaluate(&mut self, ctx: &StrategyContext<'_>) -> Result<StrategyIntent, EngineError> {
        let Some(mid) = Self::mid(ctx.orderbook) else {
            return Ok(StrategyIntent::hold(self.strategy_id()));
        };
        self.on_tick(ctx.orderbook.timestamp, mid);
        let current = ctx.account.effective_position_qty(&self.symbol);
        let Some((entry_high, entry_low)) = Self::channel(&self.completed, self.entry_n) else {
            return Ok(self.flatten_leftover(ctx, current));
        };
        let Some((exit_high, exit_low)) = Self::channel(&self.completed, self.exit_n) else {
            return Ok(self.flatten_leftover(ctx, current));
        };

        let instrument = Instrument::new(ctx.instrument_type.clone(), self.symbol.clone());
        if mid > entry_high {
            return Ok(StrategyIntent::target_position(
                self.strategy_id(),
                instrument,
                self.position_size,
            ));
        }
        if mid < entry_low {
            return Ok(StrategyIntent::target_position(
                self.strategy_id(),
                instrument,
                -self.position_size,
            ));
        }
        if current > FLAT_EPS && mid < exit_low {
            return Ok(StrategyIntent::target_position(
                self.strategy_id(),
                instrument,
                Decimal::ZERO,
            ));
        }
        if current < -FLAT_EPS && mid > exit_high {
            return Ok(StrategyIntent::target_position(
                self.strategy_id(),
                instrument,
                Decimal::ZERO,
            ));
        }
        Ok(StrategyIntent::hold(self.strategy_id()))
    }

    async fn on_execution_report(&mut self, _order: &Order) -> Result<(), EngineError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cex::intent::StrategyContext;
    use crate::cex::state::TradingState;
    use trading_core::types::{common::InstrumentType, market_data::Orderbook};

    fn book(pair: Pair, ts_ms: u64, mid: Decimal) -> Orderbook {
        Orderbook {
            pair,
            bids: vec![(mid - Decimal::ONE, Decimal::ONE)],
            asks: vec![(mid + Decimal::ONE, Decimal::ONE)],
            timestamp: ts_ms,
            update_kind: Default::default(),
            sequence: None,
        }
    }

    fn day_ms(day: u64) -> u64 {
        day * 86_400_000
    }

    #[tokio::test]
    async fn breakout_goes_long() {
        let pair = Pair::new("BTC", "USDT");
        let mut strat = Donchian::new(pair.clone(), Decimal::new(1, 3), 3, 2);
        let mut bars = Vec::new();
        for d in 0..3 {
            bars.push((
                day_ms(d),
                Decimal::new(100 + d as i64, 0),
                Decimal::new(90, 0),
                Decimal::new(95, 0),
            ));
        }
        strat.seed_daily_bars(&bars).await.unwrap();
        // Force completed-only: seed treats "today" as forming. Push via ticks on later days.
        let account = TradingState::new();
        let orderbook = book(pair.clone(), day_ms(10), Decimal::new(200, 0));
        let ctx = StrategyContext {
            orderbook: &orderbook,
            account: &account,
            instrument_type: InstrumentType::Futures,
        };
        let intent = strat.evaluate(&ctx).await.unwrap();
        match intent {
            StrategyIntent::TargetPosition { qty, .. } => {
                assert_eq!(qty, Decimal::new(1, 3));
            }
            other => panic!("expected long, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn warmup_flattens_leftover() {
        let pair = Pair::new("BTC", "USDT");
        let mut strat = Donchian::new(pair.clone(), Decimal::new(1, 3), 20, 10);
        let mut account = TradingState::new();
        account.set_position(pair.clone(), Decimal::new(5, 2), Decimal::new(100, 0));
        let orderbook = book(pair, 1, Decimal::new(100, 0));
        let ctx = StrategyContext {
            orderbook: &orderbook,
            account: &account,
            instrument_type: InstrumentType::Futures,
        };
        let intent = strat.evaluate(&ctx).await.unwrap();
        match intent {
            StrategyIntent::TargetPosition { qty, .. } => assert_eq!(qty, Decimal::ZERO),
            other => panic!("expected flatten, got {other:?}"),
        }
    }
}
