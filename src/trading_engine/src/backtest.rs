use super::strategy::Strategy;
use core_logic::{
    data::public_data::Orderbook,
    error::EngineError,
};
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Simple backtesting framework
pub struct BacktestEngine<S: Strategy> {
    strategy: Arc<RwLock<S>>,
    initial_balance: Decimal,
    balance: Decimal,
    position: Decimal,
    avg_price: Decimal,
    trades: Vec<TradeResult>,
}

#[derive(Debug, Clone)]
pub struct TradeResult {
    pub side: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub pnl: Decimal,
    pub balance_after: Decimal,
}

impl<S: Strategy> BacktestEngine<S> {
    pub fn new(strategy: Arc<RwLock<S>>, initial_balance: Decimal) -> Self {
        Self {
            strategy,
            initial_balance,
            balance: initial_balance,
            position: Decimal::ZERO,
            avg_price: Decimal::ZERO,
            trades: Vec::new(),
        }
    }

    /// Run backtest on a series of orderbooks
    pub async fn run(&mut self, orderbooks: Vec<Orderbook>) -> Result<BacktestResult, EngineError> {
        for orderbook in orderbooks {
            let state = {
                let strat = self.strategy.read().await;
                strat.get_state().clone()
            };
            
            let order_request = {
                let mut strat = self.strategy.write().await;
                strat.on_orderbook(&orderbook, &state).await?
            };

            if let Some(req) = order_request {
                // Simulate order execution at mid price
                let mid_price = if !orderbook.bids.is_empty() && !orderbook.asks.is_empty() {
                    (orderbook.bids[0] + orderbook.asks[0]) / Decimal::from(2)
                } else {
                    continue;
                };

                let cost = mid_price * req.amount;
                
                match req.side {
                    core_logic::data::base_data::Side::Buy => {
                        if self.balance >= cost {
                            self.balance -= cost;
                            let total_cost = self.avg_price * self.position + cost;
                            self.position += req.amount;
                            if self.position > Decimal::ZERO {
                                self.avg_price = total_cost / self.position;
                            }
                            
                            self.trades.push(TradeResult {
                                side: "BUY".to_string(),
                                price: mid_price,
                                quantity: req.amount,
                                pnl: Decimal::ZERO,
                                balance_after: self.balance,
                            });
                        }
                    }
                    core_logic::data::base_data::Side::Sell => {
                        if self.position >= req.amount {
                            let revenue = mid_price * req.amount;
                            let pnl = (mid_price - self.avg_price) * req.amount;
                            
                            self.balance += revenue;
                            self.position -= req.amount;
                            
                            self.trades.push(TradeResult {
                                side: "SELL".to_string(),
                                price: mid_price,
                                quantity: req.amount,
                                pnl,
                                balance_after: self.balance,
                            });
                        }
                    }
                }
            }
        }

        // Calculate final P&L
        let final_value = self.balance + self.position * self.avg_price;
        let total_pnl = final_value - self.initial_balance;
        let return_percent = (total_pnl / self.initial_balance) * Decimal::from(100);

        Ok(BacktestResult {
            initial_balance: self.initial_balance,
            final_balance: self.balance,
            final_position: self.position,
            total_pnl,
            return_percent,
            total_trades: self.trades.len(),
            trades: self.trades.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct BacktestResult {
    pub initial_balance: Decimal,
    pub final_balance: Decimal,
    pub final_position: Decimal,
    pub total_pnl: Decimal,
    pub return_percent: Decimal,
    pub total_trades: usize,
    pub trades: Vec<TradeResult>,
}

