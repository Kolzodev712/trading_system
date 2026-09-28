//! Durable economic execution ledger (JSONL, not Mongo).
//!
//! Order acknowledgements are not fills. Live PnL is reconstructed from recorded
//! executions, fees, and funding. Duplicate `execution_id` / `settlement_id` values
//! are ignored. Replay after restart must match in-process state.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir_all, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use trading_core::{
    error::EngineError,
    types::{
        common::{Pair, Side},
        exchange::Trade,
    },
};

/// One economic fill. `execution_id` is the idempotency key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FillRecord {
    pub execution_id: String,
    pub order_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_id: Option<String>,
    pub symbol_base: String,
    pub symbol_quote: String,
    pub side: String,
    pub quantity: String,
    pub price: String,
    pub fee: String,
    pub fee_currency: String,
    pub exchange_ts_ms: u64,
    pub receive_ts_ms: u64,
}

/// One funding settlement. `settlement_id` is the idempotency key.
/// `amount` is signed quote PnL: negative means paid.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FundingRecord {
    pub settlement_id: String,
    pub symbol_base: String,
    pub symbol_quote: String,
    pub amount: String,
    pub exchange_ts_ms: u64,
    pub receive_ts_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LedgerRecord {
    Fill(FillRecord),
    Funding(FundingRecord),
}

impl LedgerRecord {
    fn id(&self) -> &str {
        match self {
            Self::Fill(fill) => fill.execution_id.as_str(),
            Self::Funding(funding) => funding.settlement_id.as_str(),
        }
    }

    fn event_ts_ms(&self) -> u64 {
        match self {
            Self::Fill(fill) => fill.exchange_ts_ms,
            Self::Funding(funding) => funding.exchange_ts_ms,
        }
    }
}

/// Mark-to-market economic snapshot reconstructed from the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerSnapshot {
    pub quantity: HashMap<Pair, Decimal>,
    pub avg_price: HashMap<Pair, Decimal>,
    pub realized_price_pnl: Decimal,
    pub fees_paid: Decimal,
    pub funding_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub net_realized: Decimal,
    pub equity: Decimal,
    pub daily_pnl: Decimal,
}

impl LedgerSnapshot {
    fn empty() -> Self {
        Self {
            quantity: HashMap::new(),
            avg_price: HashMap::new(),
            realized_price_pnl: Decimal::ZERO,
            fees_paid: Decimal::ZERO,
            funding_pnl: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            net_realized: Decimal::ZERO,
            equity: Decimal::ZERO,
            daily_pnl: Decimal::ZERO,
        }
    }
}

#[derive(Default)]
struct Book {
    quantity: Decimal,
    avg_price: Decimal,
    realized_price_pnl: Decimal,
    fees_paid: Decimal,
    funding_pnl: Decimal,
}

impl Book {
    fn apply_fill(
        &mut self,
        side: Side,
        qty: Decimal,
        price: Decimal,
        fee_quote: Decimal,
    ) -> Result<(), EngineError> {
        if qty <= Decimal::ZERO {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "ledger fill quantity must be positive"
            )));
        }
        let signed = match side {
            Side::Buy => qty,
            Side::Sell => -qty,
        };
        if self.quantity.is_zero() || same_sign(self.quantity, signed) {
            let abs_new = (self.quantity + signed).abs();
            if abs_new > Decimal::ZERO {
                let cost = self.avg_price * self.quantity.abs() + price * qty;
                self.avg_price = cost / abs_new;
            }
            self.quantity += signed;
        } else {
            let closing = qty.min(self.quantity.abs());
            let pnl = if self.quantity > Decimal::ZERO {
                (price - self.avg_price) * closing
            } else {
                (self.avg_price - price) * closing
            };
            self.realized_price_pnl += pnl;
            self.quantity += signed;
            if self.quantity.is_zero() {
                self.avg_price = Decimal::ZERO;
            } else if !same_sign(self.quantity, self.quantity - signed) {
                // Reversed through flat: leftover opens at the fill price.
                self.avg_price = price;
            }
        }
        self.fees_paid += fee_quote.abs();
        Ok(())
    }

    fn apply_funding(&mut self, amount: Decimal) {
        self.funding_pnl += amount;
    }

    fn net_realized(&self) -> Decimal {
        self.realized_price_pnl - self.fees_paid + self.funding_pnl
    }

    fn unrealized(&self, mark: Decimal) -> Decimal {
        if self.quantity.is_zero() {
            Decimal::ZERO
        } else {
            (mark - self.avg_price) * self.quantity
        }
    }
}

fn same_sign(a: Decimal, b: Decimal) -> bool {
    (a > Decimal::ZERO && b > Decimal::ZERO) || (a < Decimal::ZERO && b < Decimal::ZERO)
}

/// Append-only JSONL execution ledger.
pub struct ExecutionLedger {
    path: PathBuf,
    file: Mutex<std::fs::File>,
    seen: Mutex<HashSet<String>>,
}

impl ExecutionLedger {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EngineError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent).map_err(|err| {
                    EngineError::CustomError(anyhow::anyhow!(
                        "execution ledger dir {}: {err}",
                        parent.display()
                    ))
                })?;
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|err| {
                EngineError::CustomError(anyhow::anyhow!(
                    "open execution ledger {}: {err}",
                    path.display()
                ))
            })?;
        let seen = load_seen(&path, &mut file)?;
        Ok(Self {
            path,
            file: Mutex::new(file),
            seen: Mutex::new(seen),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record_fill(&self, fill: FillRecord) -> Result<bool, EngineError> {
        self.append(LedgerRecord::Fill(fill))
    }

    pub fn record_funding(&self, funding: FundingRecord) -> Result<bool, EngineError> {
        self.append(LedgerRecord::Funding(funding))
    }

    pub fn record_trade(&self, trade: &Trade) -> Result<bool, EngineError> {
        self.record_fill(fill_from_trade(trade))
    }

    fn append(&self, record: LedgerRecord) -> Result<bool, EngineError> {
        let id = record.id().to_string();
        {
            let seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
            if seen.contains(&id) {
                return Ok(false);
            }
        }
        let mut line = serde_json::to_string(&record).map_err(|err| {
            EngineError::CustomError(anyhow::anyhow!("encode execution ledger: {err}"))
        })?;
        line.push('\n');
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        file.write_all(line.as_bytes()).map_err(|err| {
            EngineError::CustomError(anyhow::anyhow!("write execution ledger: {err}"))
        })?;
        file.flush().map_err(|err| {
            EngineError::CustomError(anyhow::anyhow!("flush execution ledger: {err}"))
        })?;
        file.sync_all().map_err(|err| {
            EngineError::CustomError(anyhow::anyhow!("fsync execution ledger: {err}"))
        })?;
        self.seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id);
        Ok(true)
    }

    pub fn replay(&self) -> Result<Vec<LedgerRecord>, EngineError> {
        let file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        drop(file);
        read_records(&self.path)
    }

    pub fn snapshot(
        &self,
        marks: &HashMap<Pair, Decimal>,
        as_of_ms: u64,
    ) -> Result<LedgerSnapshot, EngineError> {
        snapshot_from_records(&self.replay()?, marks, as_of_ms)
    }
}

fn load_seen(path: &Path, _file: &mut std::fs::File) -> Result<HashSet<String>, EngineError> {
    Ok(read_records(path)?
        .into_iter()
        .map(|r| r.id().to_string())
        .collect())
}

fn read_records(path: &Path) -> Result<Vec<LedgerRecord>, EngineError> {
    let file = OpenOptions::new().read(true).open(path).map_err(|err| {
        EngineError::CustomError(anyhow::anyhow!(
            "read execution ledger {}: {err}",
            path.display()
        ))
    })?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = line.map_err(|err| {
            EngineError::CustomError(anyhow::anyhow!("execution ledger line {}: {err}", idx + 1))
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let record: LedgerRecord = serde_json::from_str(&line).map_err(|err| {
            EngineError::CustomError(anyhow::anyhow!(
                "parse execution ledger line {}: {err}",
                idx + 1
            ))
        })?;
        records.push(record);
    }
    Ok(records)
}

pub fn fill_from_trade(trade: &Trade) -> FillRecord {
    let execution_id = if trade.trade_id.is_empty() {
        format!(
            "order:{}:{}:{}",
            trade.order_id.as_str(),
            trade.quantity,
            trade.timestamp
        )
    } else {
        trade.trade_id.to_string()
    };
    FillRecord {
        execution_id,
        order_id: trade.order_id.as_str().to_string(),
        client_order_id: None,
        strategy_id: None,
        symbol_base: trade.pair.base.to_string(),
        symbol_quote: trade.pair.quote.to_string(),
        side: format!("{:?}", trade.side),
        quantity: trade.quantity.to_string(),
        price: trade.price.to_string(),
        fee: trade.fee.to_string(),
        fee_currency: trade.fee_currency.to_string(),
        exchange_ts_ms: trade.timestamp,
        receive_ts_ms: now_ms(),
    }
}

fn parse_side(side: &str) -> Result<Side, EngineError> {
    match side.to_ascii_lowercase().as_str() {
        "buy" => Ok(Side::Buy),
        "sell" => Ok(Side::Sell),
        other => Err(EngineError::CustomError(anyhow::anyhow!(
            "ledger unknown side {other}"
        ))),
    }
}

fn parse_decimal(value: &str, field: &str) -> Result<Decimal, EngineError> {
    value
        .parse()
        .map_err(|err| EngineError::CustomError(anyhow::anyhow!("ledger {field}: {err}")))
}

fn fee_in_quote(fee: Decimal, fee_currency: &str, pair: &Pair, price: Decimal) -> Decimal {
    if fee.is_zero() {
        return Decimal::ZERO;
    }
    let currency = fee_currency.to_ascii_uppercase();
    if currency == pair.quote.to_ascii_uppercase()
        || matches!(currency.as_str(), "USDT" | "USDC" | "USD")
    {
        fee.abs()
    } else if currency == pair.base.to_ascii_uppercase() {
        fee.abs() * price
    } else {
        fee.abs()
    }
}

fn utc_date(ms: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp((ms / 1000) as i64, 0)
        .unwrap_or_else(chrono::Utc::now)
        .format("%Y-%m-%d")
        .to_string()
}

fn snapshot_from_records(
    records: &[LedgerRecord],
    marks: &HashMap<Pair, Decimal>,
    as_of_ms: u64,
) -> Result<LedgerSnapshot, EngineError> {
    let today = utc_date(as_of_ms);
    let mut books: HashMap<Pair, Book> = HashMap::new();
    let mut sod_books: HashMap<Pair, Book> = HashMap::new();
    let mut seen = HashSet::new();
    for record in records {
        if !seen.insert(record.id().to_string()) {
            continue;
        }
        if record.event_ts_ms() > as_of_ms {
            continue;
        }
        apply_record(&mut books, record)?;
        if utc_date(record.event_ts_ms()) < today {
            apply_record(&mut sod_books, record)?;
        }
    }
    Ok(finish_snapshot(books, Some(sod_books), marks))
}

fn apply_record(books: &mut HashMap<Pair, Book>, record: &LedgerRecord) -> Result<(), EngineError> {
    match record {
        LedgerRecord::Fill(fill) => {
            let pair = Pair::new(fill.symbol_base.as_str(), fill.symbol_quote.as_str());
            let qty = parse_decimal(&fill.quantity, "quantity")?;
            let price = parse_decimal(&fill.price, "price")?;
            let fee = parse_decimal(&fill.fee, "fee")?;
            let side = parse_side(&fill.side)?;
            let fee_quote = fee_in_quote(fee, &fill.fee_currency, &pair, price);
            books
                .entry(pair)
                .or_default()
                .apply_fill(side, qty, price, fee_quote)?;
        }
        LedgerRecord::Funding(funding) => {
            let pair = Pair::new(funding.symbol_base.as_str(), funding.symbol_quote.as_str());
            let amount = parse_decimal(&funding.amount, "funding")?;
            books.entry(pair).or_default().apply_funding(amount);
        }
    }
    Ok(())
}

fn finish_snapshot(
    books: HashMap<Pair, Book>,
    sod: Option<HashMap<Pair, Book>>,
    marks: &HashMap<Pair, Decimal>,
) -> LedgerSnapshot {
    let mut snapshot = LedgerSnapshot::empty();
    for (pair, book) in &books {
        let mark = marks.get(pair).copied().unwrap_or(book.avg_price);
        snapshot.quantity.insert(pair.clone(), book.quantity);
        snapshot.avg_price.insert(pair.clone(), book.avg_price);
        snapshot.realized_price_pnl += book.realized_price_pnl;
        snapshot.fees_paid += book.fees_paid;
        snapshot.funding_pnl += book.funding_pnl;
        snapshot.unrealized_pnl += book.unrealized(mark);
    }
    snapshot.net_realized = snapshot.realized_price_pnl - snapshot.fees_paid + snapshot.funding_pnl;
    snapshot.equity = snapshot.net_realized + snapshot.unrealized_pnl;
    let sod_equity = sod.map(|books| {
        let mut equity = Decimal::ZERO;
        for (pair, book) in books {
            let mark = marks.get(&pair).copied().unwrap_or(book.avg_price);
            equity += book.net_realized() + book.unrealized(mark);
        }
        equity
    });
    snapshot.daily_pnl = snapshot.equity - sod_equity.unwrap_or(Decimal::ZERO);
    snapshot
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Paper/local values are simulation. Live venues remain unlabeled as research alpha.
pub fn economic_source(venue: &str, environment: &str) -> &'static str {
    if venue.eq_ignore_ascii_case("PAPER") || environment.eq_ignore_ascii_case("LOCAL") {
        "simulation"
    } else {
        "exchange"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::sync::atomic::{AtomicU64, Ordering};
    use trading_core::types::common::{OrderId, Symbol};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn ledger_path(name: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("inc9-ledger-{n}-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("execution-ledger.jsonl")
    }

    fn pair() -> Pair {
        Pair::new("BTC", "USDT")
    }

    fn fill(
        id: &str,
        side: &str,
        qty: Decimal,
        price: Decimal,
        fee: Decimal,
        ts: u64,
    ) -> FillRecord {
        FillRecord {
            execution_id: id.into(),
            order_id: format!("ord-{id}"),
            client_order_id: Some("clid".into()),
            strategy_id: Some("C2".into()),
            symbol_base: "BTC".into(),
            symbol_quote: "USDT".into(),
            side: side.into(),
            quantity: qty.to_string(),
            price: price.to_string(),
            fee: fee.to_string(),
            fee_currency: "USDT".into(),
            exchange_ts_ms: ts,
            receive_ts_ms: ts,
        }
    }

    fn marks(price: Decimal) -> HashMap<Pair, Decimal> {
        let mut map = HashMap::new();
        map.insert(pair(), price);
        map
    }

    #[test]
    fn open_long_partial_and_full_close() {
        let path = ledger_path("open-close");
        let ledger = ExecutionLedger::open(&path).unwrap();
        ledger
            .record_fill(fill("e1", "Buy", dec!(2), dec!(100), Decimal::ZERO, 1_000))
            .unwrap();
        let open = ledger.snapshot(&marks(dec!(110)), 2_000).unwrap();
        assert_eq!(open.quantity[&pair()], dec!(2));
        assert_eq!(open.avg_price[&pair()], dec!(100));
        assert_eq!(open.unrealized_pnl, dec!(20));
        assert_eq!(open.realized_price_pnl, Decimal::ZERO);

        ledger
            .record_fill(fill("e2", "Sell", dec!(1), dec!(130), Decimal::ZERO, 3_000))
            .unwrap();
        let partial = ledger.snapshot(&marks(dec!(130)), 4_000).unwrap();
        assert_eq!(partial.quantity[&pair()], dec!(1));
        assert_eq!(partial.avg_price[&pair()], dec!(100));
        assert_eq!(partial.realized_price_pnl, dec!(30));
        assert_eq!(partial.unrealized_pnl, dec!(30));

        ledger
            .record_fill(fill("e3", "Sell", dec!(1), dec!(90), Decimal::ZERO, 5_000))
            .unwrap();
        let closed = ledger.snapshot(&marks(dec!(90)), 6_000).unwrap();
        assert_eq!(
            closed
                .quantity
                .get(&pair())
                .copied()
                .unwrap_or(Decimal::ZERO),
            Decimal::ZERO
        );
        assert_eq!(closed.realized_price_pnl, dec!(20)); // +30 then -10
        assert_eq!(closed.unrealized_pnl, Decimal::ZERO);
    }

    #[test]
    fn winning_and_losing_closes() {
        let path = ledger_path("win-loss");
        let ledger = ExecutionLedger::open(&path).unwrap();
        ledger
            .record_fill(fill("b", "Buy", dec!(1), dec!(100), Decimal::ZERO, 1))
            .unwrap();
        ledger
            .record_fill(fill("w", "Sell", dec!(1), dec!(120), Decimal::ZERO, 2))
            .unwrap();
        assert_eq!(
            ledger
                .snapshot(&HashMap::new(), 3)
                .unwrap()
                .realized_price_pnl,
            dec!(20)
        );

        ledger
            .record_fill(fill("b2", "Buy", dec!(1), dec!(100), Decimal::ZERO, 4))
            .unwrap();
        ledger
            .record_fill(fill("l", "Sell", dec!(1), dec!(80), Decimal::ZERO, 5))
            .unwrap();
        assert_eq!(
            ledger
                .snapshot(&HashMap::new(), 6)
                .unwrap()
                .realized_price_pnl,
            Decimal::ZERO
        );
    }

    #[test]
    fn fees_and_funding_are_economic() {
        let path = ledger_path("fees-funding");
        let ledger = ExecutionLedger::open(&path).unwrap();
        ledger
            .record_fill(fill("b", "Buy", dec!(1), dec!(100), dec!(0.5), 1))
            .unwrap();
        ledger
            .record_funding(FundingRecord {
                settlement_id: "f1".into(),
                symbol_base: "BTC".into(),
                symbol_quote: "USDT".into(),
                amount: "-2".into(),
                exchange_ts_ms: 2,
                receive_ts_ms: 2,
            })
            .unwrap();
        ledger
            .record_fill(fill("s", "Sell", dec!(1), dec!(110), dec!(0.5), 3))
            .unwrap();
        let snap = ledger.snapshot(&HashMap::new(), 4).unwrap();
        assert_eq!(snap.realized_price_pnl, dec!(10));
        assert_eq!(snap.fees_paid, dec!(1.0));
        assert_eq!(snap.funding_pnl, dec!(-2));
        assert_eq!(snap.net_realized, dec!(7));
        assert_eq!(snap.daily_pnl, dec!(7));
    }

    #[test]
    fn duplicate_execution_is_ignored() {
        let path = ledger_path("dup");
        let ledger = ExecutionLedger::open(&path).unwrap();
        let rec = fill("same", "Buy", dec!(1), dec!(100), Decimal::ZERO, 1);
        assert!(ledger.record_fill(rec.clone()).unwrap());
        assert!(!ledger.record_fill(rec).unwrap());
        let snap = ledger.snapshot(&marks(dec!(100)), 2).unwrap();
        assert_eq!(snap.quantity[&pair()], dec!(1));
    }

    #[test]
    fn replay_after_restart_matches() {
        let path = ledger_path("restart");
        let ledger = ExecutionLedger::open(&path).unwrap();
        ledger
            .record_fill(fill("e1", "Buy", dec!(2), dec!(50), dec!(0.1), 10))
            .unwrap();
        ledger
            .record_fill(fill("e2", "Sell", dec!(1), dec!(80), dec!(0.1), 20))
            .unwrap();
        ledger
            .record_funding(FundingRecord {
                settlement_id: "fun".into(),
                symbol_base: "BTC".into(),
                symbol_quote: "USDT".into(),
                amount: "1.5".into(),
                exchange_ts_ms: 25,
                receive_ts_ms: 25,
            })
            .unwrap();
        let before = ledger.snapshot(&marks(dec!(90)), 30).unwrap();
        drop(ledger);
        let reopened = ExecutionLedger::open(&path).unwrap();
        let after = reopened.snapshot(&marks(dec!(90)), 30).unwrap();
        assert_eq!(before, after);
        assert_eq!(after.quantity[&pair()], dec!(1));
        assert_eq!(after.avg_price[&pair()], dec!(50));
        assert_eq!(after.realized_price_pnl, dec!(30));
    }

    #[test]
    fn record_trade_uses_trade_id() {
        let path = ledger_path("trade");
        let ledger = ExecutionLedger::open(&path).unwrap();
        let trade = Trade {
            trade_id: "paper-exec-1".into(),
            order_id: OrderId::new("ord-1".into()),
            pair: pair(),
            price: dec!(100),
            quantity: dec!(0.01),
            side: Side::Buy,
            timestamp: 1_000,
            fee: dec!(0.06),
            fee_currency: Symbol::from("USDT"),
        };
        assert!(ledger.record_trade(&trade).unwrap());
        assert!(!ledger.record_trade(&trade).unwrap());
        let snap = ledger.snapshot(&marks(dec!(100)), 2_000).unwrap();
        assert_eq!(snap.quantity[&pair()], dec!(0.01));
        assert_eq!(snap.fees_paid, dec!(0.06));
    }

    #[test]
    fn damaged_record_fails_open_replay() {
        let path = ledger_path("torn");
        std::fs::write(&path, "{not json\n").unwrap();
        let err = match ExecutionLedger::open(&path) {
            Ok(_) => panic!("parse must fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("parse execution ledger"));
    }

    #[test]
    fn paper_is_simulation_source() {
        assert_eq!(economic_source("PAPER", "LOCAL"), "simulation");
        assert_eq!(economic_source("BYBIT", "DEMO"), "exchange");
    }
}
