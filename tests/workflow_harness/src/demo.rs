use anyhow::{bail, Result};
use reqwest::Client;
use std::time::Duration;

use crate::types::{Capability, ScenarioResult, Verdict};

/// Read-only Bybit Demo adapter. Never places orders, never calls Resume.
pub struct DemoAdapter {
    pub rest_base: String,
    pub capabilities: Capability,
}

impl DemoAdapter {
    pub fn new() -> Result<Self> {
        if std::env::var("WORKFLOW_ALLOW_BYBIT_MAINNET")
            .ok()
            .as_deref()
            == Some("1")
        {
            bail!("refusing WORKFLOW_ALLOW_BYBIT_MAINNET: this harness must not target mainnet");
        }
        if std::env::var("WORKFLOW_ALLOW_BYBIT_TESTNET")
            .ok()
            .as_deref()
            == Some("1")
        {
            bail!("refusing WORKFLOW_ALLOW_BYBIT_TESTNET: Demo only, no silent testnet fallback");
        }
        Ok(Self {
            rest_base: "https://api-demo.bybit.com".into(),
            capabilities: Capability::demo_readonly(),
        })
    }

    pub async fn readonly_checks(&self) -> ScenarioResult {
        let mut r = ScenarioResult::new("demo_readonly_paused", "BYBIT", "Spot", "paused");
        r.note("read-only Demo adapter; no mutating RPCs");
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("client");
        let time_url = format!("{}/v5/market/time", self.rest_base);
        match client.get(&time_url).send().await {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                r.assert_eq("demo_host", "api-demo.bybit.com", {
                    if time_url.contains("api-demo.bybit.com")
                        && !time_url.contains("testnet")
                        && !time_url.contains("api.bybit.com/v5")
                    {
                        "api-demo.bybit.com"
                    } else {
                        "wrong-host"
                    }
                });
                r.assert_true(
                    "market_time_http",
                    status.is_success(),
                    "2xx",
                    &status.to_string(),
                );
                r.note(format!("market/time body_len={}", body.len()));
            }
            Err(e) => r.fail("market_time", "reachable Demo public API", &e.to_string()),
        }

        let wallet = client
            .get(format!("{}/v5/account/wallet-balance", self.rest_base))
            .query(&[("accountType", "UNIFIED")])
            .send()
            .await;
        match wallet {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                r.note(format!("wallet-balance without keys status={status}"));
                let denied =
                    !status.is_success() || body.contains("retCode") || body.contains("API key");
                r.assert_eq(
                    "auth_denial",
                    "denied",
                    if denied {
                        "denied"
                    } else {
                        "unexpected_success"
                    },
                );
            }
            Err(e) => r.note(format!(
                "wallet-balance error (acceptable for readonly): {e}"
            )),
        }

        if !self.capabilities.mutating {
            r.assert_eq("no_mutating_flag", "false", "false");
        }
        r
    }
}

pub fn mutating_refused() -> ScenarioResult {
    let mut r = ScenarioResult::new("demo_mutating_preflight", "BYBIT", "Spot", "opt-in");
    let allowed = std::env::var("WORKFLOW_DEMO_MUTATE").ok().as_deref() == Some("1");
    if !allowed {
        r.note("WORKFLOW_DEMO_MUTATE is not set; refusing to place Demo orders");
        r.fail(
            "explicit_mutate_flag",
            "WORKFLOW_DEMO_MUTATE=1 required; this command still places no orders in this task",
            "flag unset; no Resume, no order, no live funds",
        );
        return r;
    }
    r.verdict = Verdict::NotSupported;
    r.note("flag present but this task does not Resume or place Demo orders");
    r.assertions.push(crate::types::AssertionRecord {
        name: "no_demo_orders".into(),
        expected: "no mutating Demo calls".into(),
        actual: "NOT_SUPPORTED (preflight only)".into(),
        verdict: Verdict::NotSupported,
    });
    r
}

impl Default for DemoAdapter {
    fn default() -> Self {
        Self::new().expect("demo adapter")
    }
}
