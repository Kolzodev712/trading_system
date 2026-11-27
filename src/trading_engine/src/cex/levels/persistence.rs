//! Level Manager persistence to MongoDB
use crate::cex::levels::config::{
    LevelConfig, LevelRiskConfig, LifecycleConfig, OrderTypeConfig, PriceConfig, QuantityConfig,
};
use crate::risk::RiskLimits;
use mongodb::bson::doc;
use smol_str::SmolStr;
use trading_core::error::EngineError;
use trading_core::persistence::mongo::{
    client::MongoClient, collections::BOT_CONFIG_COLLECTION, models::*,
};

/// Level Manager persistence
pub struct LevelPersistence {
    mongo_client: MongoClient,
    bot_id: smol_str::SmolStr,
}

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("MongoDB error: {0}")]
    MongoError(#[from] trading_core::persistence::mongo::client::MongoError),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Engine error: {0}")]
    EngineError(#[from] EngineError),
}

impl LevelPersistence {
    /// Create a new LevelPersistence instance
    pub fn new(mongo_client: MongoClient, bot_id: impl Into<smol_str::SmolStr>) -> Self {
        Self {
            mongo_client,
            bot_id: bot_id.into(),
        }
    }

    /// Save levels configuration and risk limits to MongoDB
    pub async fn save_levels(
        &self,
        levels: &[LevelConfig],
        risk_limits: &RiskLimits,
        configured_pairs: &[trading_core::types::common::Pair],
    ) -> Result<(), PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<mongodb::bson::Document>(BOT_CONFIG_COLLECTION);

        // Convert levels to document format
        let level_docs: Vec<LevelConfigDocument> = levels
            .iter()
            .map(|level| self.level_config_to_document(level))
            .collect::<Result<Vec<_>, _>>()?;

        // Convert risk limits to document format
        let risk_limits_doc = RiskLimitsDocument {
            total_max_position: risk_limits.total_max_position,
            max_total_orders: risk_limits.max_total_orders,
            max_notional: risk_limits.max_notional,
            max_order_qty: risk_limits.max_order_qty,
            daily_loss_limit_all_pairs: risk_limits.daily_loss_limit_all_pairs,
            daily_loss_limit_per_pair: risk_limits
                .daily_loss_limit_per_pair
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
        };

        // Get configured pairs from parameter (from bot config)
        let configured_pairs: Vec<String> = configured_pairs
            .iter()
            .map(|p| format!("{}{}", p.base, p.quote))
            .collect();

        let trading_config = TradingConfigDocument { configured_pairs };

        // Create or update bot config document
        let bot_config_doc = BotConfigDocument {
            bot_id: self.bot_id.as_str().to_string(),
            levels: level_docs,
            risk_limits: risk_limits_doc,
            trading_config,
            updated_at: chrono::Utc::now().timestamp() as u64,
        };

        // Serialize to BSON document
        let bson_doc = mongodb::bson::to_document(&bot_config_doc).map_err(|e| {
            PersistenceError::EngineError(EngineError::CustomError(smol_str::format_smolstr!(
                "BSON conversion error: {}",
                e
            )))
        })?;

        // Upsert (insert or update)
        let filter = doc! { "_id": self.bot_id.as_str() };
        let update = doc! { "$set": bson_doc };

        let options = mongodb::options::UpdateOptions::builder()
            .upsert(true)
            .build();
        collection
            .update_one(filter, update)
            .with_options(options)
            .await
            .map_err(|e| {
                PersistenceError::MongoError(
                    trading_core::persistence::mongo::client::MongoError::Error(e),
                )
            })?;

        Ok(())
    }

    /// Load levels configuration and risk limits from MongoDB
    pub async fn load_levels(&self) -> Result<Option<BotConfigDocument>, PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<BotConfigDocument>(BOT_CONFIG_COLLECTION);

        let filter = doc! { "_id": self.bot_id.as_str() };
        let result = collection.find_one(filter).await.map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })?;

        Ok(result)
    }

    /// Update a single level in MongoDB
    pub async fn update_level(&self, level: &LevelConfig) -> Result<(), PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<mongodb::bson::Document>(BOT_CONFIG_COLLECTION);

        let level_doc = self.level_config_to_document(level)?;
        let bson_doc = mongodb::bson::to_document(&level_doc).map_err(|e| {
            PersistenceError::EngineError(EngineError::CustomError(smol_str::format_smolstr!(
                "BSON conversion error: {}",
                e
            )))
        })?;

        let filter = doc! {
            "_id": self.bot_id.as_str(),
            "levels.id": level.id.as_str()
        };

        let update = doc! {
            "$set": {
                "levels.$[level]": bson_doc,
                "updated_at": chrono::Utc::now().timestamp()
            }
        };

        let array_filters = vec![doc! { "level.id": level.id.as_str() }];
        let options = mongodb::options::UpdateOptions::builder()
            .array_filters(array_filters)
            .build();

        collection
            .update_one(filter, update)
            .with_options(options)
            .await
            .map_err(|e| {
                PersistenceError::MongoError(
                    trading_core::persistence::mongo::client::MongoError::Error(e),
                )
            })?;

        Ok(())
    }

    /// Delete a level from MongoDB
    pub async fn delete_level(&self, level_id: &str) -> Result<(), PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<mongodb::bson::Document>(BOT_CONFIG_COLLECTION);

        let filter = doc! { "_id": self.bot_id.as_str() };
        let update = doc! {
            "$pull": {
                "levels": { "id": level_id }
            },
            "$set": {
                "updated_at": chrono::Utc::now().timestamp()
            }
        };

        collection.update_one(filter, update).await.map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })?;

        Ok(())
    }

    /// Save risk limits to MongoDB
    pub async fn save_risk_limits(&self, risk_limits: &RiskLimits) -> Result<(), PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<mongodb::bson::Document>(BOT_CONFIG_COLLECTION);

        let risk_limits_doc = RiskLimitsDocument {
            total_max_position: risk_limits.total_max_position,
            max_total_orders: risk_limits.max_total_orders,
            max_notional: risk_limits.max_notional,
            max_order_qty: risk_limits.max_order_qty,
            daily_loss_limit_all_pairs: risk_limits.daily_loss_limit_all_pairs,
            daily_loss_limit_per_pair: risk_limits
                .daily_loss_limit_per_pair
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
        };

        let bson_doc = mongodb::bson::to_document(&risk_limits_doc).map_err(|e| {
            PersistenceError::EngineError(EngineError::CustomError(smol_str::format_smolstr!(
                "BSON conversion error: {}",
                e
            )))
        })?;

        let filter = doc! { "_id": self.bot_id.as_str() };
        let update = doc! {
            "$set": {
                "risk_limits": bson_doc,
                "updated_at": chrono::Utc::now().timestamp()
            }
        };

        collection.update_one(filter, update).await.map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })?;

        Ok(())
    }

    /// Convert LevelConfig to LevelConfigDocument
    fn level_config_to_document(
        &self,
        level: &LevelConfig,
    ) -> Result<LevelConfigDocument, PersistenceError> {
        // Serialize quantity and price configs to JSON
        let quantity_json = serde_json::to_value(&level.quantity)?;
        let price_json = serde_json::to_value(&level.price)?;

        Ok(LevelConfigDocument {
            id: level.id.to_string(),
            name: level.name.as_ref().map(|n| n.to_string()),
            // Note: symbol is deprecated - levels are now pair-agnostic
            // Store empty pair for backward compatibility
            symbol: PairDocument {
                base: String::new(),
                quote: String::new(),
            },
            side: format!("{:?}", level.side),
            order_type: format!("{:?}", level.order_type),
            quantity: quantity_json,
            price: price_json,
            risk: LevelRiskConfigDocument {
                max_position: level.risk.max_position,
                max_orders: level.risk.max_orders,
                daily_loss_limit: level.risk.daily_loss_limit,
            },
            lifecycle: LifecycleConfigDocument {
                auto_repost: level.lifecycle.auto_repost,
                auto_repost_on_cancel: level.lifecycle.auto_repost_on_cancel,
                max_reposts: level.lifecycle.max_reposts,
                repost_delay_secs: level.lifecycle.repost_delay_secs,
            },
            enabled: level.enabled,
            group: level.group.as_ref().map(|g| g.to_string()),
        })
    }

    /// Convert LevelConfigDocument to LevelConfig
    pub fn document_to_level_config(
        &self,
        doc: &LevelConfigDocument,
    ) -> Result<LevelConfig, PersistenceError> {
        // Deserialize quantity and price from JSON
        let quantity: QuantityConfig =
            serde_json::from_value(doc.quantity.clone()).map_err(|e| {
                PersistenceError::EngineError(EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to deserialize quantity: {}",
                    e
                )))
            })?;
        let price: PriceConfig = serde_json::from_value(doc.price.clone()).map_err(|e| {
            PersistenceError::EngineError(EngineError::CustomError(smol_str::format_smolstr!(
                "Failed to deserialize price: {}",
                e
            )))
        })?;

        // Parse side
        let side = match doc.side.as_str() {
            "Buy" => trading_core::types::common::Side::Buy,
            "Sell" => trading_core::types::common::Side::Sell,
            _ => {
                return Err(PersistenceError::EngineError(EngineError::CustomError(
                    smol_str::format_smolstr!("Invalid side: {}", doc.side),
                )));
            }
        };

        // Parse order type
        let order_type = match doc.order_type.as_str() {
            "Market" => OrderTypeConfig::Market,
            "Limit" => OrderTypeConfig::Limit,
            "PostOnly" => OrderTypeConfig::PostOnly,
            "IoC" => OrderTypeConfig::IoC,
            "FoK" => OrderTypeConfig::FoK,
            _ => {
                return Err(PersistenceError::EngineError(EngineError::CustomError(
                    smol_str::format_smolstr!("Invalid order type: {}", doc.order_type),
                )));
            }
        };

        // Note: symbol is deprecated - levels are now pair-agnostic
        // Ignore symbol from document (will be expanded per pair when loading)

        Ok(LevelConfig {
            id: SmolStr::new(&doc.id),
            name: doc.name.as_ref().map(SmolStr::new),
            side,
            order_type,
            quantity,
            price,
            risk: LevelRiskConfig {
                max_position: doc.risk.max_position,
                max_orders: doc.risk.max_orders,
                max_order_qty: None,
                max_notional: None,
                daily_loss_limit: doc.risk.daily_loss_limit,
            },
            lifecycle: LifecycleConfig {
                auto_repost: doc.lifecycle.auto_repost,
                auto_repost_on_cancel: doc.lifecycle.auto_repost_on_cancel,
                max_reposts: doc.lifecycle.max_reposts,
                repost_delay_secs: doc.lifecycle.repost_delay_secs,
                repost_conditions: Vec::new(), // Not stored in document, default to empty
            },
            enabled: doc.enabled,
            group: doc.group.as_ref().map(SmolStr::new),
        })
    }
}
