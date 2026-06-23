//! DEX-specific panel renderers (venue-agnostic labels).

use crate::app::App;
use crate::client::trading_cli::{
    DexIndicator, DexOrder, DexQuote, DexStrategyStatus, DexVenueSnapshot,
};
use crate::error::CliError;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

/// Last segment of a Move coin type or mint address for display.
pub fn asset_label(mint: &str) -> String {
    mint.rsplit("::").next().unwrap_or(mint).to_string()
}

pub fn resolve_asset_label(
    mint: &str,
    labels: &std::collections::HashMap<String, String>,
) -> String {
    labels
        .get(mint)
        .cloned()
        .unwrap_or_else(|| asset_label(mint))
}

fn truncate_middle(value: &str, head: usize, tail: usize) -> String {
    if value.len() <= head + tail + 3 {
        return value.to_string();
    }
    format!(
        "{}…{}",
        &value[..head],
        &value[value.len().saturating_sub(tail)..]
    )
}

pub fn render_venue(frame: &mut Frame, area: Rect, app: &App) -> Result<(), CliError> {
    let (venue, exchange) = app
        .monitoring_data()
        .and_then(|md| md.dex.as_ref())
        .map(|d| (d.venue.as_str(), app.exchange_name()))
        .unwrap_or(("—", "—"));

    let lines = vec![
        Line::from(vec![
            Span::styled("Venue: ", Style::default().fg(Color::DarkGray)),
            Span::styled(venue, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Exchange: ", Style::default().fg(Color::DarkGray)),
            Span::styled(exchange, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Mode: ", Style::default().fg(Color::DarkGray)),
            Span::styled("DEX monitor", Style::default().fg(Color::Green)),
        ]),
    ];

    let block = Block::default().borders(Borders::ALL).title("Venue");
    frame.render_widget(Paragraph::new(lines).block(block), area);
    Ok(())
}

pub fn render_dex_indicators(
    frame: &mut Frame,
    area: Rect,
    dex: Option<&DexVenueSnapshot>,
    pool_filter: Option<&str>,
) -> Result<(), CliError> {
    let indicators: Vec<&DexIndicator> = if let Some(snapshot) = dex {
        snapshot
            .dex_indicators
            .iter()
            .filter(|i| {
                pool_filter
                    .map(|p| indicator_matches_pool(i, p))
                    .unwrap_or(true)
            })
            .collect()
    } else {
        Vec::new()
    };

    let rows = if indicators.is_empty() {
        vec![Row::new(vec![Cell::from("No DEX indicators")])]
    } else {
        indicators
            .iter()
            .take(8)
            .map(|i| {
                Row::new(vec![
                    Cell::from(i.name.as_str()),
                    Cell::from(i.pool.as_str()),
                    Cell::from(i.value.as_str()),
                ])
            })
            .collect()
    };

    let title = pool_filter
        .map(|p| format!("DEX Indicators ({p})"))
        .unwrap_or_else(|| "DEX Indicators".to_string());

    let table = Table::new(
        rows,
        [
            ratatui::layout::Constraint::Length(10),
            ratatui::layout::Constraint::Length(12),
            ratatui::layout::Constraint::Min(10),
        ],
    )
    .header(Row::new(vec![
        Cell::from("Kind").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Pool").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Value").style(Style::default().add_modifier(Modifier::BOLD)),
    ]))
    .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(table, area);
    Ok(())
}

fn indicator_matches_pool(indicator: &DexIndicator, pool_filter: &str) -> bool {
    if indicator.pool == pool_filter {
        return true;
    }
    let normalized = pool_filter.replace('/', "_");
    indicator.pool == normalized || indicator.pool.contains(pool_filter)
}

pub fn render_dex_balances(
    frame: &mut Frame,
    area: Rect,
    dex: Option<&DexVenueSnapshot>,
) -> Result<(), CliError> {
    let rows = if let Some(snapshot) = dex {
        if snapshot.balances.is_empty() {
            vec![Row::new(vec![Cell::from("No venue balances")])]
        } else {
            snapshot
                .balances
                .iter()
                .take(12)
                .map(|b| {
                    Row::new(vec![
                        Cell::from(resolve_asset_label(&b.mint, &snapshot.asset_labels)),
                        Cell::from(b.amount.as_str()),
                    ])
                })
                .collect()
        }
    } else {
        vec![Row::new(vec![Cell::from("No DEX data")])]
    };

    let table = Table::new(
        rows,
        [
            ratatui::layout::Constraint::Percentage(40),
            ratatui::layout::Constraint::Percentage(60),
        ],
    )
    .header(Row::new(vec![
        Cell::from("Asset").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Amount").style(Style::default().add_modifier(Modifier::BOLD)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Venue Balances"),
    );
    frame.render_widget(table, area);
    Ok(())
}

pub fn render_pending_transactions(
    frame: &mut Frame,
    area: Rect,
    dex: Option<&DexVenueSnapshot>,
) -> Result<(), CliError> {
    let rows = if let Some(snapshot) = dex {
        if snapshot.pending_transactions.is_empty() {
            vec![Row::new(vec![Cell::from("No pending transactions")])]
        } else {
            snapshot
                .pending_transactions
                .iter()
                .take(10)
                .map(|tx| {
                    Row::new(vec![
                        Cell::from(truncate_middle(&tx.signature, 10, 8)),
                        Cell::from(tx.status.as_str()),
                    ])
                })
                .collect()
        }
    } else {
        vec![Row::new(vec![Cell::from("No DEX data")])]
    };

    let table = Table::new(
        rows,
        [
            ratatui::layout::Constraint::Percentage(65),
            ratatui::layout::Constraint::Percentage(35),
        ],
    )
    .header(Row::new(vec![
        Cell::from("Signature").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().add_modifier(Modifier::BOLD)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Pending Transactions"),
    );
    frame.render_widget(table, area);
    Ok(())
}

pub fn render_dex_open_orders(
    frame: &mut Frame,
    area: Rect,
    dex: Option<&DexVenueSnapshot>,
    pool_filter: Option<&str>,
) -> Result<(), CliError> {
    let rows = if let Some(snapshot) = dex {
        let orders: Vec<&DexOrder> = snapshot
            .open_orders
            .iter()
            .filter(|o| {
                pool_filter
                    .map(|p| order_matches_pool(o, p))
                    .unwrap_or(true)
            })
            .take(15)
            .collect();

        if orders.is_empty() {
            vec![Row::new(vec![Cell::from("No open orders")])]
        } else {
            orders
                .into_iter()
                .map(|o| {
                    Row::new(vec![
                        Cell::from(truncate_middle(&o.order_id, 8, 6)),
                        Cell::from(o.pool.as_str()),
                        Cell::from(o.side.as_str()),
                        Cell::from(o.price.as_str()),
                        Cell::from(o.quantity.as_str()),
                        Cell::from(o.status.as_str()),
                    ])
                })
                .collect()
        }
    } else {
        vec![Row::new(vec![Cell::from("No DEX data")])]
    };

    let title = pool_filter
        .map(|p| format!("Open Orders ({p})"))
        .unwrap_or_else(|| "Open Orders".to_string());

    let table = Table::new(
        rows,
        [
            ratatui::layout::Constraint::Length(14),
            ratatui::layout::Constraint::Length(12),
            ratatui::layout::Constraint::Length(6),
            ratatui::layout::Constraint::Length(10),
            ratatui::layout::Constraint::Length(8),
            ratatui::layout::Constraint::Length(10),
        ],
    )
    .header(Row::new(vec![
        Cell::from("Order ID"),
        Cell::from("Pool"),
        Cell::from("Side"),
        Cell::from("Price"),
        Cell::from("Qty"),
        Cell::from("Status"),
    ]))
    .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(table, area);
    Ok(())
}

fn order_matches_pool(order: &DexOrder, pool_filter: &str) -> bool {
    if order.pool == pool_filter {
        return true;
    }
    let normalized = pool_filter.replace('/', "_");
    order.pool == normalized || order.pool.contains(pool_filter)
}

pub fn render_dex_quotes(
    frame: &mut Frame,
    area: Rect,
    dex: Option<&DexVenueSnapshot>,
    pool_filter: Option<&str>,
    selected_index: Option<usize>,
) -> Result<(), CliError> {
    let quotes: Vec<&DexQuote> = if let Some(snapshot) = dex {
        snapshot
            .quotes
            .iter()
            .filter(|q| {
                pool_filter
                    .map(|p| quote_matches_pool(q, p))
                    .unwrap_or(true)
            })
            .collect()
    } else {
        Vec::new()
    };

    let rows = if quotes.is_empty() {
        vec![Row::new(vec![Cell::from("No configured quotes")])]
    } else {
        quotes
            .iter()
            .enumerate()
            .map(|(idx, q)| {
                let style = if selected_index == Some(idx) {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                let enabled = if q.enabled { "on" } else { "off" };
                let price_cell = if q.resolved_price.is_empty() {
                    q.price.as_str()
                } else {
                    q.resolved_price.as_str()
                };
                Row::new(vec![
                    Cell::from(q.id.as_str()).style(style),
                    Cell::from(q.side.as_str()).style(style),
                    Cell::from(price_cell).style(style),
                    Cell::from(q.quantity.as_str()).style(style),
                    Cell::from(enabled).style(style),
                    Cell::from(q.state.as_str()).style(style),
                ])
            })
            .collect()
    };

    let title = pool_filter
        .map(|p| format!("Quotes ({p})"))
        .unwrap_or_else(|| "Quotes".to_string());

    let table = Table::new(
        rows,
        [
            ratatui::layout::Constraint::Length(10),
            ratatui::layout::Constraint::Length(6),
            ratatui::layout::Constraint::Length(10),
            ratatui::layout::Constraint::Length(8),
            ratatui::layout::Constraint::Length(5),
            ratatui::layout::Constraint::Min(10),
        ],
    )
    .header(Row::new(vec![
        Cell::from("ID").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Side").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Price").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Qty").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("En").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("State").style(Style::default().add_modifier(Modifier::BOLD)),
    ]))
    .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(table, area);
    Ok(())
}

fn quote_matches_pool(quote: &DexQuote, pool_filter: &str) -> bool {
    if quote.pool == pool_filter {
        return true;
    }
    let normalized = pool_filter.replace('/', "_");
    quote.pool == normalized || quote.pool.contains(pool_filter)
}

pub fn render_dex_strategies(
    frame: &mut Frame,
    area: Rect,
    dex: Option<&DexVenueSnapshot>,
    pool_filter: Option<&str>,
    selected_index: Option<usize>,
) -> Result<(), CliError> {
    let strategies: Vec<&DexStrategyStatus> = if let Some(snapshot) = dex {
        snapshot
            .dex_strategies
            .iter()
            .filter(|s| {
                pool_filter
                    .map(|p| strategy_matches_market(s, p))
                    .unwrap_or(true)
            })
            .collect()
    } else {
        Vec::new()
    };

    let rows = if strategies.is_empty() {
        vec![Row::new(vec![Cell::from("No configured strategies")])]
    } else {
        strategies
            .iter()
            .enumerate()
            .map(|(idx, s)| {
                let style = if selected_index == Some(idx) {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                let enabled = if s.enabled { "on" } else { "off" };
                Row::new(vec![
                    Cell::from(s.id.as_str()).style(style),
                    Cell::from(s.strategy_type.as_str()).style(style),
                    Cell::from(s.market.as_str()).style(style),
                    Cell::from(enabled).style(style),
                    Cell::from(s.state.as_str()).style(style),
                    Cell::from(truncate_middle(&s.last_signal, 12, 8)).style(style),
                ])
            })
            .collect()
    };

    let title = pool_filter
        .map(|p| format!("Strategies ({p})"))
        .unwrap_or_else(|| "Strategies".to_string());

    let table = Table::new(
        rows,
        [
            ratatui::layout::Constraint::Length(14),
            ratatui::layout::Constraint::Length(8),
            ratatui::layout::Constraint::Length(12),
            ratatui::layout::Constraint::Length(5),
            ratatui::layout::Constraint::Length(10),
            ratatui::layout::Constraint::Min(10),
        ],
    )
    .header(Row::new(vec![
        Cell::from("ID").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Type").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Market").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("En").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("State").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Signal").style(Style::default().add_modifier(Modifier::BOLD)),
    ]))
    .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(table, area);
    Ok(())
}

fn strategy_matches_market(strategy: &DexStrategyStatus, pool_filter: &str) -> bool {
    if strategy.market == pool_filter {
        return true;
    }
    let normalized = pool_filter.replace('/', "_");
    strategy.market == normalized || strategy.market.contains(pool_filter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_label_uses_last_segment() {
        assert_eq!(asset_label("0x2::sui::SUI"), "SUI");
        assert_eq!(asset_label("0xf715::DBUSDC::DBUSDC"), "DBUSDC");
    }
}
