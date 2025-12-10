//! Strategy persistence to MongoDB
use futures_util::StreamExt;
use mongodb::bson::doc;
use trading_core::error::EngineError;
use trading_core::persistence::mongo::{
    client::MongoClient, collections::STRATEGIES_COLLECTION, models::*,
};

/// Strategy persistence
pub struct StrategyPersistence {
    mongo_client: MongoClient,
    #[allow(dead_code)] // Reserved for future multi-bot filtering
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

impl StrategyPersistence {
    /// Create a new StrategyPersistence instance
    pub fn new(mongo_client: MongoClient, bot_id: impl Into<smol_str::SmolStr>) -> Self {
        Self {
            mongo_client,
            bot_id: bot_id.into(),
        }
    }

    /// Save strategy configuration to MongoDB
    pub async fn save_strategy(
        &self,
        strategy_id: &str,
        config: &StrategyConfigDocument,
    ) -> Result<(), PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<mongodb::bson::Document>(STRATEGIES_COLLECTION);

        let now = chrono::Utc::now().timestamp() as u64;
        let mut strategy_doc = config.clone();
        strategy_doc.strategy_id = strategy_id.to_string();
        strategy_doc.updated_at = now;
        if strategy_doc.created_at == 0 {
            strategy_doc.created_at = now;
        }

        let bson_doc = mongodb::bson::to_document(&strategy_doc).map_err(|e| {
            PersistenceError::EngineError(EngineError::CustomError(anyhow::anyhow!(
                "BSON conversion error: {}",
                e
            )))
        })?;
        let filter = doc! { "_id": strategy_id };
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

    /// Load strategy configuration from MongoDB
    pub async fn load_strategy(
        &self,
        strategy_id: &str,
    ) -> Result<Option<StrategyConfigDocument>, PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<StrategyConfigDocument>(STRATEGIES_COLLECTION);

        let filter = doc! { "_id": strategy_id };
        let result = collection.find_one(filter).await.map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })?;

        Ok(result)
    }

    /// Load all strategies for this bot
    pub async fn load_all_strategies(
        &self,
    ) -> Result<Vec<StrategyConfigDocument>, PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<StrategyConfigDocument>(STRATEGIES_COLLECTION);

        // Load all strategies (StrategyConfigDocument doesn't include bot_id field)
        // If multi-bot filtering is needed, add bot_id to StrategyConfigDocument
        let mut cursor = collection.find(doc! {}).await.map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })?;
        let mut strategies = Vec::new();

        while let Some(doc) = cursor.next().await.transpose().map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })? {
            strategies.push(doc);
        }

        Ok(strategies)
    }

    /// Update strategy configuration in MongoDB
    pub async fn update_strategy(
        &self,
        strategy_id: &str,
        config: &StrategyConfigDocument,
    ) -> Result<(), PersistenceError> {
        // Update is same as save (upsert)
        self.save_strategy(strategy_id, config).await
    }

    /// Delete strategy from MongoDB
    pub async fn delete_strategy(&self, strategy_id: &str) -> Result<(), PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<mongodb::bson::Document>(STRATEGIES_COLLECTION);

        let filter = doc! { "_id": strategy_id };
        collection.delete_one(filter).await.map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })?;

        Ok(())
    }
}
