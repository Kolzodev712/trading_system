use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    Pass,
    Fail,
    NotSupported,
    NotRun,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::NotSupported => "NOT_SUPPORTED",
            Self::NotRun => "NOT_RUN",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Paper,
    Ui,
    DemoReadonly,
    DemoMutating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    Spot,
    Futures,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub force_fill: bool,
    pub fault_inject: bool,
    pub rest_positions: bool,
    pub mutating: bool,
}

impl Capability {
    pub fn paper() -> Self {
        Self {
            force_fill: true,
            fault_inject: true,
            rest_positions: true,
            mutating: true,
        }
    }

    pub fn demo_readonly() -> Self {
        Self {
            force_fill: false,
            fault_inject: false,
            rest_positions: true,
            mutating: false,
        }
    }

    pub fn demo_mutating() -> Self {
        Self {
            force_fill: false,
            fault_inject: false,
            rest_positions: true,
            mutating: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionRecord {
    pub name: String,
    pub expected: String,
    pub actual: String,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub workflow: String,
    pub venue: String,
    pub instrument: String,
    pub execution_mode: String,
    pub expected_failure: bool,
    pub verdict: Verdict,
    pub first_failure: Option<AssertionRecord>,
    pub assertions: Vec<AssertionRecord>,
    pub timeline: Vec<String>,
}

impl ScenarioResult {
    pub fn new(workflow: &str, venue: &str, instrument: &str, execution_mode: &str) -> Self {
        Self {
            workflow: workflow.into(),
            venue: venue.into(),
            instrument: instrument.into(),
            execution_mode: execution_mode.into(),
            expected_failure: false,
            verdict: Verdict::Pass,
            first_failure: None,
            assertions: Vec::new(),
            timeline: Vec::new(),
        }
    }

    pub fn not_supported(mut self, why: &str) -> Self {
        self.verdict = Verdict::NotSupported;
        self.timeline.push(format!("NOT_SUPPORTED: {why}"));
        self
    }

    pub fn not_run(mut self, why: &str) -> Self {
        self.verdict = Verdict::NotRun;
        self.timeline.push(format!("NOT_RUN: {why}"));
        self
    }

    pub fn note(&mut self, line: impl Into<String>) {
        self.timeline.push(line.into());
    }

    pub fn assert_eq(&mut self, name: &str, expected: impl ToString, actual: impl ToString) {
        let expected = expected.to_string();
        let actual = actual.to_string();
        let verdict = if expected == actual {
            Verdict::Pass
        } else {
            Verdict::Fail
        };
        let rec = AssertionRecord {
            name: name.into(),
            expected,
            actual,
            verdict,
        };
        if verdict == Verdict::Fail && self.first_failure.is_none() {
            self.first_failure = Some(rec.clone());
            self.verdict = Verdict::Fail;
        }
        self.assertions.push(rec);
    }

    pub fn assert_true(&mut self, name: &str, ok: bool, expected: &str, actual: &str) {
        if ok {
            self.assert_eq(name, "true", "true");
        } else {
            self.assert_eq(name, expected, actual);
        }
    }

    pub fn fail(&mut self, name: &str, expected: &str, actual: &str) {
        self.assert_eq(name, expected, actual);
    }

    pub fn mark_not_run(&mut self, name: &str, why: &str) {
        self.assertions.push(AssertionRecord {
            name: name.into(),
            expected: "run".into(),
            actual: why.into(),
            verdict: Verdict::NotRun,
        });
        if self.verdict == Verdict::Pass {
            self.verdict = Verdict::NotRun;
        }
    }

    pub fn gate_ok(&self) -> bool {
        match self.verdict {
            Verdict::Pass => !self.expected_failure,
            Verdict::Fail => self.expected_failure,
            // Spec: never convert an unrun or unsupported scenario into a pass.
            Verdict::NotSupported | Verdict::NotRun => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowMatrix {
    pub run_id: String,
    pub artifact_dir: PathBuf,
    pub layers: Vec<String>,
    pub scenarios: Vec<ScenarioResult>,
}

impl WorkflowMatrix {
    pub fn print_human(&self) {
        println!(
            "RUN {} artifacts {}",
            self.run_id,
            self.artifact_dir.display()
        );
        println!(
            "{:<42} {:<8} {:<10} {:<10} {:<14}",
            "workflow", "venue", "instrument", "mode", "verdict"
        );
        for s in &self.scenarios {
            println!(
                "{:<42} {:<8} {:<10} {:<10} {:<14}",
                s.workflow, s.venue, s.instrument, s.execution_mode, s.verdict
            );
            if let Some(f) = &s.first_failure {
                println!(
                    "  first broken: {} expected={} actual={}",
                    f.name, f.expected, f.actual
                );
                for line in s
                    .timeline
                    .iter()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                {
                    println!("    {line}");
                }
            }
        }
    }

    pub fn overall_ok(&self) -> bool {
        self.scenarios.iter().all(|s| s.gate_ok())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("{0}")]
    Fail(String),
    #[error("NOT_SUPPORTED: {0}")]
    NotSupported(String),
}

impl From<anyhow::Error> for WorkflowError {
    fn from(e: anyhow::Error) -> Self {
        Self::Fail(e.to_string())
    }
}
