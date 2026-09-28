//! Bybit open-order REST pagination and linear selector rules.
//!
//! Isolated from HTTP so page-merge, cursor, and selector failures can be tested
//! without a live venue. Truncating a remaining cursor is an error, not a short list.

use super::common_types::BybitCategory;
use smol_str::SmolStr;
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::future::Future;

/// Bybit `limit` for `GET /v5/order/realtime` is `1..=50`.
pub const BYBIT_OPEN_ORDER_PAGE_LIMIT: u32 = 50;

/// Hard stop so a stuck cursor cannot loop forever. 40 pages × 50 = 2000 orders.
pub const BYBIT_OPEN_ORDER_MAX_PAGES: usize = 40;

/// One REST page: items plus optional `nextPageCursor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderPage<T> {
    pub items: Vec<T>,
    pub next_page_cursor: Option<String>,
}

/// Failures that must not be collapsed into a silently short open-order list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenOrderPaginationError {
    InvalidLimit { limit: u32 },
    MissingLinearSelector,
    PageFailed { page: usize, source: String },
    RepeatedCursor { cursor: String },
    Truncated { pages: usize, limit: u32 },
}

impl Display for OpenOrderPaginationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLimit { limit } => {
                write!(
                    f,
                    "Bybit open-order limit {limit} is outside 1..={BYBIT_OPEN_ORDER_PAGE_LIMIT}"
                )
            }
            Self::MissingLinearSelector => write!(
                f,
                "Bybit linear/inverse open-order query requires symbol, baseCoin, or settleCoin"
            ),
            Self::PageFailed { page, source } => {
                write!(f, "Bybit open-order page {page} failed: {source}")
            }
            Self::RepeatedCursor { cursor } => {
                write!(f, "Bybit open-order pagination repeated cursor {cursor}")
            }
            Self::Truncated { pages, limit } => write!(
                f,
                "Bybit open-order list truncated after {pages} pages (limit {limit})"
            ),
        }
    }
}

impl std::error::Error for OpenOrderPaginationError {}

/// Item identity used to drop duplicates that appear on more than one page.
pub trait OpenOrderPageItem {
    fn page_item_id(&self) -> &str;
}

/// Linear/inverse list queries must include exactly one venue selector family.
pub fn validate_open_order_selector(
    category: BybitCategory,
    symbol: Option<&str>,
    base_coin: Option<&str>,
    settle_coin: Option<&str>,
) -> Result<(), OpenOrderPaginationError> {
    match category {
        BybitCategory::Linear | BybitCategory::Inverse => {
            let has_symbol = symbol.map(|s| !s.is_empty()).unwrap_or(false);
            let has_base = base_coin.map(|s| !s.is_empty()).unwrap_or(false);
            let has_settle = settle_coin.map(|s| !s.is_empty()).unwrap_or(false);
            if has_symbol || has_base || has_settle {
                Ok(())
            } else {
                Err(OpenOrderPaginationError::MissingLinearSelector)
            }
        }
        BybitCategory::Spot | BybitCategory::Option => Ok(()),
    }
}

/// Selector fields for a linear open-order or history query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearListSelector {
    pub symbol: Option<SmolStr>,
    pub base_coin: Option<SmolStr>,
    pub settle_coin: Option<SmolStr>,
}

/// Prefer a concrete `symbol` when one pair is configured; otherwise `settleCoin`.
pub fn linear_list_selector(
    category: BybitCategory,
    symbol: Option<SmolStr>,
    settle_coin: SmolStr,
) -> Result<LinearListSelector, OpenOrderPaginationError> {
    match category {
        BybitCategory::Linear | BybitCategory::Inverse => {
            let selector = if let Some(symbol) = symbol.filter(|s| !s.is_empty()) {
                LinearListSelector {
                    symbol: Some(symbol),
                    base_coin: None,
                    settle_coin: None,
                }
            } else {
                LinearListSelector {
                    symbol: None,
                    base_coin: None,
                    settle_coin: Some(settle_coin),
                }
            };
            validate_open_order_selector(
                category,
                selector.symbol.as_deref(),
                selector.base_coin.as_deref(),
                selector.settle_coin.as_deref(),
            )?;
            Ok(selector)
        }
        BybitCategory::Spot | BybitCategory::Option => Ok(LinearListSelector {
            symbol: None,
            base_coin: None,
            settle_coin: None,
        }),
    }
}

/// Walk `nextPageCursor` until empty. Duplicates are dropped by item id. A remaining
/// cursor after `max_pages`, a repeated cursor, or a page fetch error fails the whole list.
pub async fn collect_open_order_pages<T, E, F, Fut>(
    mut fetch_page: F,
    limit: u32,
    max_pages: usize,
) -> Result<Vec<T>, OpenOrderPaginationError>
where
    T: OpenOrderPageItem,
    F: FnMut(Option<String>) -> Fut,
    Fut: Future<Output = Result<OrderPage<T>, E>>,
    E: Display,
{
    if !(1..=BYBIT_OPEN_ORDER_PAGE_LIMIT).contains(&limit) {
        return Err(OpenOrderPaginationError::InvalidLimit { limit });
    }
    if max_pages == 0 {
        return Err(OpenOrderPaginationError::Truncated { pages: 0, limit });
    }

    let mut merged = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_cursors = HashSet::new();
    let mut cursor: Option<String> = None;

    for page_idx in 0..max_pages {
        if let Some(ref current) = cursor {
            if !seen_cursors.insert(current.clone()) {
                return Err(OpenOrderPaginationError::RepeatedCursor {
                    cursor: current.clone(),
                });
            }
        }

        let page = fetch_page(cursor.clone()).await.map_err(|err| {
            OpenOrderPaginationError::PageFailed {
                page: page_idx,
                source: err.to_string(),
            }
        })?;

        for item in page.items {
            if seen_ids.insert(item.page_item_id().to_string()) {
                merged.push(item);
            }
        }

        match page.next_page_cursor.filter(|c| !c.is_empty()) {
            None => return Ok(merged),
            Some(next) => {
                if cursor.as_ref() == Some(&next) {
                    return Err(OpenOrderPaginationError::RepeatedCursor { cursor: next });
                }
                cursor = Some(next);
            }
        }
    }

    Err(OpenOrderPaginationError::Truncated {
        pages: max_pages,
        limit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Item(String);

    impl OpenOrderPageItem for Item {
        fn page_item_id(&self) -> &str {
            &self.0
        }
    }

    fn item(id: &str) -> Item {
        Item(id.to_string())
    }

    fn items(ids: &[&str]) -> Vec<Item> {
        ids.iter().map(|id| item(id)).collect()
    }

    async fn collect(
        pages: Vec<Result<OrderPage<Item>, String>>,
        limit: u32,
        max_pages: usize,
    ) -> Result<Vec<Item>, OpenOrderPaginationError> {
        let mut remaining = pages.into_iter();
        collect_open_order_pages(
            move |_cursor| {
                let next = remaining.next().unwrap_or_else(|| {
                    Ok(OrderPage {
                        items: vec![],
                        next_page_cursor: None,
                    })
                });
                async move { next }
            },
            limit,
            max_pages,
        )
        .await
    }

    #[tokio::test]
    async fn zero_orders() {
        let pages = vec![Ok(OrderPage {
            items: vec![],
            next_page_cursor: None,
        })];
        let got = collect(pages, 50, 10).await.expect("empty list");
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn one_order() {
        let pages = vec![Ok(OrderPage {
            items: items(&["a"]),
            next_page_cursor: None,
        })];
        let got = collect(pages, 50, 10).await.expect("one order");
        assert_eq!(got, items(&["a"]));
    }

    #[tokio::test]
    async fn fifty_orders_single_full_page() {
        let ids: Vec<String> = (0..50).map(|i| format!("o{i}")).collect();
        let page_items: Vec<Item> = ids.iter().cloned().map(Item).collect();
        let pages = vec![Ok(OrderPage {
            items: page_items.clone(),
            next_page_cursor: None,
        })];
        let got = collect(pages, 50, 10).await.expect("50 orders");
        assert_eq!(got, page_items);
    }

    #[tokio::test]
    async fn fifty_one_orders_two_pages() {
        let first: Vec<Item> = (0..50).map(|i| item(&format!("o{i}"))).collect();
        let pages = vec![
            Ok(OrderPage {
                items: first,
                next_page_cursor: Some("c2".into()),
            }),
            Ok(OrderPage {
                items: items(&["o50"]),
                next_page_cursor: None,
            }),
        ];
        let got = collect(pages, 50, 10).await.expect("51 orders");
        assert_eq!(got.len(), 51);
        assert_eq!(got[0], item("o0"));
        assert_eq!(got[50], item("o50"));
    }

    #[tokio::test]
    async fn more_than_one_hundred_orders() {
        let mut pages = Vec::new();
        for page in 0..3 {
            let ids: Vec<Item> = (0..50).map(|i| item(&format!("p{page}-{i}"))).collect();
            let next = if page < 2 {
                Some(format!("c{}", page + 1))
            } else {
                None
            };
            pages.push(Ok(OrderPage {
                items: ids,
                next_page_cursor: next,
            }));
        }
        let got = collect(pages, 50, 10).await.expect(">100 orders");
        assert_eq!(got.len(), 150);
        assert_eq!(got[0], item("p0-0"));
        assert_eq!(got[149], item("p2-49"));
    }

    #[tokio::test]
    async fn repeated_cursor_is_an_error() {
        let pages = vec![
            Ok(OrderPage {
                items: items(&["a"]),
                next_page_cursor: Some("loop".into()),
            }),
            Ok(OrderPage {
                items: items(&["b"]),
                next_page_cursor: Some("loop".into()),
            }),
            Ok(OrderPage {
                items: items(&["c"]),
                next_page_cursor: Some("loop".into()),
            }),
        ];
        let err = collect(pages, 50, 10).await.expect_err("repeated cursor");
        assert!(matches!(
            err,
            OpenOrderPaginationError::RepeatedCursor { cursor } if cursor == "loop"
        ));
    }

    #[tokio::test]
    async fn duplicate_order_across_pages_is_deduped() {
        let pages = vec![
            Ok(OrderPage {
                items: items(&["a", "b"]),
                next_page_cursor: Some("c2".into()),
            }),
            Ok(OrderPage {
                items: items(&["b", "c"]),
                next_page_cursor: None,
            }),
        ];
        let got = collect(pages, 50, 10).await.expect("dedupe");
        assert_eq!(got, items(&["a", "b", "c"]));
    }

    #[tokio::test]
    async fn page_error_fails_the_whole_list() {
        let pages = vec![
            Ok(OrderPage {
                items: items(&["a"]),
                next_page_cursor: Some("c2".into()),
            }),
            Err("timeout".into()),
        ];
        let err = collect(pages, 50, 10).await.expect_err("page error");
        assert!(matches!(
            err,
            OpenOrderPaginationError::PageFailed { page: 1, source } if source == "timeout"
        ));
    }

    #[tokio::test]
    async fn remaining_cursor_after_max_pages_is_truncated() {
        let pages = vec![Ok(OrderPage {
            items: items(&["a"]),
            next_page_cursor: Some("still-more".into()),
        })];
        let err = collect(pages, 50, 1).await.expect_err("truncated");
        assert!(matches!(
            err,
            OpenOrderPaginationError::Truncated {
                pages: 1,
                limit: 50
            }
        ));
    }

    #[tokio::test]
    async fn limit_zero_is_rejected() {
        let err = collect_open_order_pages(
            |_c| async {
                Ok::<_, Infallible>(OrderPage::<Item> {
                    items: vec![],
                    next_page_cursor: None,
                })
            },
            0,
            10,
        )
        .await
        .expect_err("limit 0");
        assert!(matches!(
            err,
            OpenOrderPaginationError::InvalidLimit { limit: 0 }
        ));
    }

    #[test]
    fn linear_without_selector_is_invalid() {
        let err = validate_open_order_selector(BybitCategory::Linear, None, None, None)
            .expect_err("linear requires selector");
        assert_eq!(err, OpenOrderPaginationError::MissingLinearSelector);
    }

    #[test]
    fn linear_settle_coin_is_valid() {
        validate_open_order_selector(BybitCategory::Linear, None, None, Some("USDT"))
            .expect("settleCoin");
    }

    #[test]
    fn linear_symbol_is_valid() {
        validate_open_order_selector(BybitCategory::Linear, Some("BTCUSDT"), None, None)
            .expect("symbol");
    }

    #[test]
    fn spot_without_selector_is_valid() {
        validate_open_order_selector(BybitCategory::Spot, None, None, None).expect("spot");
    }

    #[test]
    fn linear_selector_prefers_symbol() {
        let selector = linear_list_selector(
            BybitCategory::Linear,
            Some(SmolStr::new("BTCUSDT")),
            SmolStr::new("USDT"),
        )
        .expect("selector");
        assert_eq!(selector.symbol.as_deref(), Some("BTCUSDT"));
        assert!(selector.settle_coin.is_none());
    }

    #[test]
    fn linear_selector_falls_back_to_settle_coin() {
        let selector = linear_list_selector(BybitCategory::Linear, None, SmolStr::new("USDT"))
            .expect("selector");
        assert_eq!(selector.settle_coin.as_deref(), Some("USDT"));
    }
}
