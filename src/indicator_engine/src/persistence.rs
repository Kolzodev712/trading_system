//! Indicator Engine persistence to MongoDB
use crate::indicators::traits::{IndicatorConfig, PriceSource};
use futures_util::StreamExt;
use mongodb::bson::doc;
use smol_str::SmolStr;
use trading_core::error::EngineError;
use trading_core::persistence::mongo::{
    client::MongoClient,
    collections::INDICATORS_COLLECTION,
    models::{CompositeIndicatorDocument, SimpleIndicatorDocument},
};

/// Indicator Engine persistence
pub struct IndicatorPersistence {
    mongo_client: MongoClient,
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

impl IndicatorPersistence {
    /// Create a new IndicatorPersistence instance
    pub fn new(mongo_client: MongoClient) -> Self {
        Self { mongo_client }
    }

    /// Save a simple indicator to MongoDB
    pub async fn save_simple_indicator(
        &self,
        config: &IndicatorConfig,
    ) -> Result<(), PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<SimpleIndicatorDocument>(INDICATORS_COLLECTION);

        let now = chrono::Utc::now().timestamp() as i64;
        let indicator_doc = SimpleIndicatorDocument {
            indicator_id: config.id.to_string(),
            indicator_type: "simple".to_string(),
            name: config.name.to_string(),
            pair: config.pair.to_string(),
            period: config.period as u32,
            source: format!("{:?}", config.source),
            created_at: now as u64,
            updated_at: now as u64,
        };

        let filter = doc! { "_id": &indicator_doc.indicator_id };
        let update_doc = mongodb::bson::to_document(&indicator_doc).map_err(|e| {
            PersistenceError::EngineError(EngineError::CustomError(anyhow::anyhow!(
                "BSON conversion error: {}",
                e
            )))
        })?;
        let update = doc! { "$set": update_doc };

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

    /// Save a composite indicator to MongoDB
    pub async fn save_composite_indicator(
        &self,
        indicator: &CompositeIndicatorDocument,
    ) -> Result<(), PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<CompositeIndicatorDocument>(INDICATORS_COLLECTION);

        let filter = doc! { "_id": &indicator.indicator_id };
        let update_doc = mongodb::bson::to_document(indicator).map_err(|e| {
            PersistenceError::EngineError(EngineError::CustomError(anyhow::anyhow!(
                "BSON conversion error: {}",
                e
            )))
        })?;
        let update = doc! { "$set": update_doc };

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

    /// Load all indicators from MongoDB
    pub async fn load_all_indicators(&self) -> Result<Vec<IndicatorConfig>, PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<SimpleIndicatorDocument>(INDICATORS_COLLECTION);

        let mut cursor = collection
            .find(doc! { "indicator_type": "simple" })
            .await
            .map_err(|e| {
                PersistenceError::MongoError(
                    trading_core::persistence::mongo::client::MongoError::Error(e),
                )
            })?;
        let mut indicators = Vec::new();

        while let Some(doc) = cursor.next().await.transpose().map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })? {
            let config = IndicatorConfig {
                id: SmolStr::new(&doc.indicator_id),
                name: SmolStr::new(&doc.name),
                pair: SmolStr::new(&doc.pair),
                period: doc.period as usize,
                source: self.parse_price_source(&doc.source)?,
            };
            indicators.push(config);
        }

        Ok(indicators)
    }

    /// Load indicator by ID
    pub async fn load_indicator(
        &self,
        indicator_id: &str,
    ) -> Result<Option<IndicatorConfig>, PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<SimpleIndicatorDocument>(INDICATORS_COLLECTION);

        let filter = doc! { "_id": indicator_id, "indicator_type": "simple" };
        let result = collection.find_one(filter).await.map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })?;

        if let Some(doc) = result {
            let config = IndicatorConfig {
                id: SmolStr::new(&doc.indicator_id),
                name: SmolStr::new(&doc.name),
                pair: SmolStr::new(&doc.pair),
                period: doc.period as usize,
                source: self.parse_price_source(&doc.source)?,
            };
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    /// Update indicator in MongoDB
    pub async fn update_indicator(&self, config: &IndicatorConfig) -> Result<(), PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<mongodb::bson::Document>(INDICATORS_COLLECTION);

        let now = chrono::Utc::now().timestamp();
        let update = doc! {
            "$set": {
                "name": config.name.to_string(),
                "pair": config.pair.to_string(),
                "period": config.period as u32,
                "source": format!("{:?}", config.source),
                "updated_at": now as i64
            }
        };

        let filter = doc! { "_id": config.id.as_str() };
        collection.update_one(filter, update).await.map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })?;

        Ok(())
    }

    /// Delete indicator from MongoDB
    pub async fn delete_indicator(&self, indicator_id: &str) -> Result<(), PersistenceError> {
        let collection = self
            .mongo_client
            .get_collection::<mongodb::bson::Document>(INDICATORS_COLLECTION);

        let filter = doc! { "_id": indicator_id };
        collection.delete_one(filter).await.map_err(|e| {
            PersistenceError::MongoError(
                trading_core::persistence::mongo::client::MongoError::Error(e),
            )
        })?;

        Ok(())
    }

    /// Parse price source from string
    fn parse_price_source(&self, source: &str) -> Result<PriceSource, PersistenceError> {
        match source {
            "Close" => Ok(PriceSource::Close),
            "Open" => Ok(PriceSource::Open),
            "High" => Ok(PriceSource::High),
            "Low" => Ok(PriceSource::Low),
            "Volume" => Ok(PriceSource::Volume),
            "TypicalPrice" => Ok(PriceSource::TypicalPrice),
            "WeightedPrice" => Ok(PriceSource::WeightedPrice),
            _ => Err(PersistenceError::EngineError(EngineError::CustomError(
                anyhow::anyhow!("Unknown price source: {}", source),
            ))),
        }
    }
}
