//! MongoDB client wrapper
use mongodb::{Client, Database};
use thiserror::Error;

/// MongoDB client wrapper
#[derive(Clone)]
pub struct MongoClient {
    client: Client,
    database: String,
}

#[derive(Debug, Error)]
pub enum MongoError {
    #[error("MongoDB error: {0}")]
    Error(#[from] mongodb::error::Error),
}

impl MongoClient {
    /// Create a new MongoDB client
    pub async fn new(connection_string: &str, database: &str) -> Result<Self, MongoError> {
        let client = Client::with_uri_str(connection_string).await?;
        Ok(Self {
            client,
            database: database.to_string(),
        })
    }

    /// Get a collection
    pub fn get_collection<T: Send + Sync>(&self, name: &str) -> mongodb::Collection<T> {
        self.database().collection(name)
    }

    /// Get the database
    pub fn database(&self) -> Database {
        self.client.database(&self.database)
    }

    /// Ping the database to test connection
    pub async fn ping(&self) -> Result<(), MongoError> {
        self.client
            .database("admin")
            .run_command(mongodb::bson::doc! { "ping": 1 })
            .await?;
        Ok(())
    }

    /// Test the connection
    pub async fn test_connection(&self) -> Result<(), MongoError> {
        self.ping().await
    }
}
