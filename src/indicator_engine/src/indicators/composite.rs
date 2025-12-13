//! Composite/Complex Indicators
//!
//! Composite indicators are formula-based indicators composed from other indicators.
//! They support arithmetic operations (+, -, *, /), functions (min, max, abs, sqrt),
//! and conditionals (if-else).
//!
//! Composite indicators depend on simple indicators (not the other way around),
//! preventing circular dependencies.
//!
//! **Note on Update Frequency**: Composite indicators recalculate when any
//! dependency indicator updates. This ensures real-time accuracy. The recalculation
//! happens immediately when a dependency value changes, not on a fixed interval.

use crate::data::PricePoint;
use crate::indicators::traits::{Indicator, IndicatorError};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Composite indicator expression
///
/// Supports:
/// - Arithmetic: +, -, *, /
/// - Functions: min, max, abs, sqrt
/// - Conditionals: if-else
/// - References to simple indicators
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum IndicatorExpression {
    /// Reference to a simple indicator by ID
    Simple { indicator_id: SmolStr },

    /// Constant value
    Constant { value: Decimal },

    /// Addition: expr1 + expr2
    Add {
        left: Box<IndicatorExpression>,
        right: Box<IndicatorExpression>,
    },

    /// Subtraction: expr1 - expr2
    Subtract {
        left: Box<IndicatorExpression>,
        right: Box<IndicatorExpression>,
    },

    /// Multiplication: expr1 * expr2
    Multiply {
        left: Box<IndicatorExpression>,
        right: Box<IndicatorExpression>,
    },

    /// Division: expr1 / expr2
    Divide {
        left: Box<IndicatorExpression>,
        right: Box<IndicatorExpression>,
    },

    /// Minimum of two expressions
    Min {
        left: Box<IndicatorExpression>,
        right: Box<IndicatorExpression>,
    },

    /// Maximum of two expressions
    Max {
        left: Box<IndicatorExpression>,
        right: Box<IndicatorExpression>,
    },

    /// Absolute value
    Abs { expr: Box<IndicatorExpression> },

    /// Square root
    Sqrt { expr: Box<IndicatorExpression> },

    /// Conditional: if condition then true_expr else false_expr
    /// Condition is evaluated: > 0 means true, <= 0 means false
    If {
        condition: Box<IndicatorExpression>,
        true_expr: Box<IndicatorExpression>,
        false_expr: Box<IndicatorExpression>,
    },
}

/// Composite indicator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeIndicatorConfig {
    /// Unique identifier for this composite indicator
    pub id: SmolStr,

    /// Display name (e.g., "Golden Cross")
    pub name: SmolStr,

    /// Trading pair
    pub pair: SmolStr,

    /// Expression tree for calculating the indicator
    pub expression: IndicatorExpression,

    /// List of simple indicator IDs this composite indicator depends on
    pub dependencies: Vec<SmolStr>,
}

impl CompositeIndicatorConfig {
    /// Validate that all dependencies are simple indicators (not composite)
    /// and that there are no circular dependencies
    pub fn validate_dependencies(&self, _all_indicators: &[SmolStr]) -> Result<(), SmolStr> {
        // TODO: Implement validation when composite indicators are fully implemented
        // - Check that all dependencies exist
        // - Check that dependencies are simple indicators (not composite)
        // - Check for circular dependencies
        Ok(())
    }

    /// Extract all dependency indicator IDs from the expression
    pub fn extract_dependencies(&self) -> Vec<SmolStr> {
        self.extract_dependencies_from_expr(&self.expression)
    }

    #[allow(clippy::only_used_in_recursion)]
    fn extract_dependencies_from_expr(&self, expr: &IndicatorExpression) -> Vec<SmolStr> {
        match expr {
            IndicatorExpression::Simple { indicator_id } => vec![indicator_id.clone()],
            IndicatorExpression::Constant { .. } => vec![],
            IndicatorExpression::Add { left, right }
            | IndicatorExpression::Subtract { left, right }
            | IndicatorExpression::Multiply { left, right }
            | IndicatorExpression::Divide { left, right }
            | IndicatorExpression::Min { left, right }
            | IndicatorExpression::Max { left, right } => {
                let mut deps = self.extract_dependencies_from_expr(left);
                deps.extend(self.extract_dependencies_from_expr(right));
                deps
            }
            IndicatorExpression::Abs { expr } | IndicatorExpression::Sqrt { expr } => {
                self.extract_dependencies_from_expr(expr)
            }
            IndicatorExpression::If {
                condition,
                true_expr,
                false_expr,
            } => {
                let mut deps = self.extract_dependencies_from_expr(condition);
                deps.extend(self.extract_dependencies_from_expr(true_expr));
                deps.extend(self.extract_dependencies_from_expr(false_expr));
                deps
            }
        }
    }
}

/// Composite indicator implementation
///
/// Calculates values based on expression tree and dependency indicators.
/// Recalculates when any dependency indicator updates.
pub struct CompositeIndicator {
    config: CompositeIndicatorConfig,
    dependency_values: std::collections::HashMap<SmolStr, Decimal>,
    cached_value: Option<Decimal>,
    is_ready: bool,
}

impl CompositeIndicator {
    /// Create a new composite indicator
    pub fn new(config: CompositeIndicatorConfig) -> Result<Self, IndicatorError> {
        // Validate that expression is not empty
        if config.dependencies.is_empty() {
            return Err(IndicatorError::CalculationError(SmolStr::new(
                "Composite indicator must have at least one dependency",
            )));
        }

        Ok(Self {
            config,
            dependency_values: std::collections::HashMap::new(),
            cached_value: None,
            is_ready: false,
        })
    }

    /// Update dependency value (called when a dependency indicator updates)
    pub fn update_dependency(&mut self, indicator_id: &SmolStr, value: Decimal) {
        if self.config.dependencies.contains(indicator_id) {
            self.dependency_values.insert(indicator_id.clone(), value);
            self.recalculate();
        }
    }

    /// Recalculate the composite indicator value
    fn recalculate(&mut self) {
        // Check if all dependencies have values
        let all_ready = self
            .config
            .dependencies
            .iter()
            .all(|dep_id| self.dependency_values.contains_key(dep_id));

        if !all_ready {
            self.is_ready = false;
            self.cached_value = None;
            return;
        }

        // Evaluate expression
        match self.evaluate_expression(&self.config.expression) {
            Some(value) => {
                self.cached_value = Some(value);
                self.is_ready = true;
            }
            None => {
                self.is_ready = false;
                self.cached_value = None;
            }
        }
    }

    /// Evaluate an expression tree
    fn evaluate_expression(&self, expr: &IndicatorExpression) -> Option<Decimal> {
        match expr {
            IndicatorExpression::Simple { indicator_id } => {
                self.dependency_values.get(indicator_id).copied()
            }
            IndicatorExpression::Constant { value } => Some(*value),
            IndicatorExpression::Add { left, right } => {
                let left_val = self.evaluate_expression(left)?;
                let right_val = self.evaluate_expression(right)?;
                Some(left_val + right_val)
            }
            IndicatorExpression::Subtract { left, right } => {
                let left_val = self.evaluate_expression(left)?;
                let right_val = self.evaluate_expression(right)?;
                Some(left_val - right_val)
            }
            IndicatorExpression::Multiply { left, right } => {
                let left_val = self.evaluate_expression(left)?;
                let right_val = self.evaluate_expression(right)?;
                Some(left_val * right_val)
            }
            IndicatorExpression::Divide { left, right } => {
                let left_val = self.evaluate_expression(left)?;
                let right_val = self.evaluate_expression(right)?;
                if right_val == Decimal::ZERO {
                    return None;
                }
                Some(left_val / right_val)
            }
            IndicatorExpression::Min { left, right } => {
                let left_val = self.evaluate_expression(left)?;
                let right_val = self.evaluate_expression(right)?;
                Some(left_val.min(right_val))
            }
            IndicatorExpression::Max { left, right } => {
                let left_val = self.evaluate_expression(left)?;
                let right_val = self.evaluate_expression(right)?;
                Some(left_val.max(right_val))
            }
            IndicatorExpression::Abs { expr } => {
                let val = self.evaluate_expression(expr)?;
                Some(val.abs())
            }
            IndicatorExpression::Sqrt { expr } => {
                let val = self.evaluate_expression(expr)?;
                if val < Decimal::ZERO {
                    return None;
                }
                // Simple sqrt approximation using f64
                // For production, consider using a proper decimal sqrt library
                let val_f64 = val.to_f64().unwrap_or(0.0);
                if val_f64 < 0.0 {
                    return None;
                }
                Some(Decimal::try_from(val_f64.sqrt()).unwrap_or(Decimal::ZERO))
            }
            IndicatorExpression::If {
                condition,
                true_expr,
                false_expr,
            } => {
                let cond_val = self.evaluate_expression(condition)?;
                if cond_val > Decimal::ZERO {
                    self.evaluate_expression(true_expr)
                } else {
                    self.evaluate_expression(false_expr)
                }
            }
        }
    }
}

impl Indicator for CompositeIndicator {
    fn id(&self) -> &str {
        self.config.id.as_str()
    }

    fn name(&self) -> &str {
        self.config.name.as_str()
    }

    fn pair(&self) -> &str {
        self.config.pair.as_str()
    }

    fn period(&self) -> usize {
        // Composite indicators don't have a traditional period
        // Return number of dependencies as a proxy
        self.config.dependencies.len()
    }

    fn update(&mut self, _price_point: PricePoint) -> Result<(), IndicatorError> {
        // Composite indicators don't update from price points directly
        // They update when dependencies update via update_dependency()
        Ok(())
    }

    fn value(&self) -> Option<Decimal> {
        self.cached_value
    }

    fn is_ready(&self) -> bool {
        self.is_ready
    }

    fn reset(&mut self) {
        self.dependency_values.clear();
        self.cached_value = None;
        self.is_ready = false;
    }

    fn config(&self) -> crate::indicators::traits::IndicatorConfig {
        crate::indicators::traits::IndicatorConfig {
            id: self.config.id.clone(),
            name: self.config.name.clone(),
            pair: self.config.pair.clone(),
            period: self.period(),
            source: crate::indicators::traits::PriceSource::Close, // Not applicable for composite
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composite_config_extract_dependencies() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("golden_cross"),
            name: SmolStr::new("Golden Cross"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Subtract {
                left: Box::new(IndicatorExpression::Simple {
                    indicator_id: SmolStr::new("ema_50"),
                }),
                right: Box::new(IndicatorExpression::Simple {
                    indicator_id: SmolStr::new("ema_200"),
                }),
            },
            dependencies: vec![SmolStr::new("ema_50"), SmolStr::new("ema_200")],
        };

        let deps = config.extract_dependencies();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&SmolStr::new("ema_50")));
        assert!(deps.contains(&SmolStr::new("ema_200")));
    }

    #[test]
    fn test_composite_indicator_new() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Constant {
                value: Decimal::from(100),
            },
            dependencies: vec![SmolStr::new("sma_20")],
        };

        let composite = CompositeIndicator::new(config).unwrap();
        assert_eq!(composite.id(), "test");
        assert!(!composite.is_ready());
    }

    #[test]
    fn test_composite_indicator_no_dependencies() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Constant {
                value: Decimal::from(100),
            },
            dependencies: vec![],
        };

        assert!(CompositeIndicator::new(config).is_err());
    }

    #[test]
    fn test_composite_indicator_arithmetic() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Add {
                left: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(10),
                }),
                right: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(20),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();

        // Update with dummy dependency (not used in expression, but required)
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        // Expression should evaluate to 30
        assert!(composite.is_ready());
        assert_eq!(composite.value().unwrap(), Decimal::from(30));
    }

    #[test]
    fn test_composite_indicator_subtract() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Subtract {
                left: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(50),
                }),
                right: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(30),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        assert_eq!(composite.value().unwrap(), Decimal::from(20));
    }

    #[test]
    fn test_composite_indicator_multiply() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Multiply {
                left: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(5),
                }),
                right: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(4),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        assert_eq!(composite.value().unwrap(), Decimal::from(20));
    }

    #[test]
    fn test_composite_indicator_divide() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Divide {
                left: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(20),
                }),
                right: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(4),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        assert_eq!(composite.value().unwrap(), Decimal::from(5));
    }

    #[test]
    fn test_composite_indicator_divide_by_zero() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Divide {
                left: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(20),
                }),
                right: Box::new(IndicatorExpression::Constant {
                    value: Decimal::ZERO,
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        assert!(!composite.is_ready());
        assert_eq!(composite.value(), None);
    }

    #[test]
    fn test_composite_indicator_min() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Min {
                left: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(10),
                }),
                right: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(20),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        assert_eq!(composite.value().unwrap(), Decimal::from(10));
    }

    #[test]
    fn test_composite_indicator_max() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Max {
                left: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(10),
                }),
                right: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(20),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        assert_eq!(composite.value().unwrap(), Decimal::from(20));
    }

    #[test]
    fn test_composite_indicator_abs() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Abs {
                expr: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(-10),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        assert_eq!(composite.value().unwrap(), Decimal::from(10));
    }

    #[test]
    fn test_composite_indicator_sqrt() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Sqrt {
                expr: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(16),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        let value = composite.value().unwrap();
        // sqrt(16) = 4 (approximately)
        assert!(value >= Decimal::from(3) && value <= Decimal::from(5));
    }

    #[test]
    fn test_composite_indicator_sqrt_negative() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Sqrt {
                expr: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(-10),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        assert!(!composite.is_ready());
        assert_eq!(composite.value(), None);
    }

    #[test]
    fn test_composite_indicator_if_true() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::If {
                condition: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(1),
                }), // true
                true_expr: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(100),
                }),
                false_expr: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(200),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        assert_eq!(composite.value().unwrap(), Decimal::from(100));
    }

    #[test]
    fn test_composite_indicator_if_false() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::If {
                condition: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(-1),
                }), // false
                true_expr: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(100),
                }),
                false_expr: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(200),
                }),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));

        assert_eq!(composite.value().unwrap(), Decimal::from(200));
    }

    #[test]
    fn test_composite_indicator_with_dependencies() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("golden_cross"),
            name: SmolStr::new("Golden Cross"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Subtract {
                left: Box::new(IndicatorExpression::Simple {
                    indicator_id: SmolStr::new("ema_50"),
                }),
                right: Box::new(IndicatorExpression::Simple {
                    indicator_id: SmolStr::new("ema_200"),
                }),
            },
            dependencies: vec![SmolStr::new("ema_50"), SmolStr::new("ema_200")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();

        // Initially not ready
        assert!(!composite.is_ready());

        // Update first dependency
        composite.update_dependency(&SmolStr::new("ema_50"), Decimal::from(100));
        assert!(!composite.is_ready()); // Still missing ema_200

        // Update second dependency
        composite.update_dependency(&SmolStr::new("ema_200"), Decimal::from(80));
        assert!(composite.is_ready());
        assert_eq!(composite.value().unwrap(), Decimal::from(20)); // 100 - 80
    }

    #[test]
    fn test_composite_indicator_reset() {
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("test"),
            name: SmolStr::new("Test"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Constant {
                value: Decimal::from(100),
            },
            dependencies: vec![SmolStr::new("dummy")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("dummy"), Decimal::from(1));
        assert!(composite.is_ready());

        composite.reset();
        assert!(!composite.is_ready());
        assert_eq!(composite.value(), None);
    }

    #[test]
    fn test_composite_indicator_complex_expression() {
        // Test: (ema_50 - ema_200) * 2 + 10
        let config = CompositeIndicatorConfig {
            id: SmolStr::new("complex"),
            name: SmolStr::new("Complex"),
            pair: SmolStr::new("BTCUSDT"),
            expression: IndicatorExpression::Add {
                left: Box::new(IndicatorExpression::Multiply {
                    left: Box::new(IndicatorExpression::Subtract {
                        left: Box::new(IndicatorExpression::Simple {
                            indicator_id: SmolStr::new("ema_50"),
                        }),
                        right: Box::new(IndicatorExpression::Simple {
                            indicator_id: SmolStr::new("ema_200"),
                        }),
                    }),
                    right: Box::new(IndicatorExpression::Constant {
                        value: Decimal::from(2),
                    }),
                }),
                right: Box::new(IndicatorExpression::Constant {
                    value: Decimal::from(10),
                }),
            },
            dependencies: vec![SmolStr::new("ema_50"), SmolStr::new("ema_200")],
        };

        let mut composite = CompositeIndicator::new(config).unwrap();
        composite.update_dependency(&SmolStr::new("ema_50"), Decimal::from(100));
        composite.update_dependency(&SmolStr::new("ema_200"), Decimal::from(80));

        // (100 - 80) * 2 + 10 = 20 * 2 + 10 = 50
        assert_eq!(composite.value().unwrap(), Decimal::from(50));
    }
}
