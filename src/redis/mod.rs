use chrono::Utc;
use redis::RedisError;
use redis::{Commands, Connection};
use serde::Serialize;

trait Redis {
    fn create_connection(&self) -> Result<Connection, redis::RedisError>;
}

pub struct RedisClass {}

impl Redis for RedisClass {
    fn create_connection(&self) -> Result<Connection, redis::RedisError> {
        let client = redis::Client::open("redis://127.0.0.1/")?;
        let connection = client.get_connection()?;

        Ok(connection)
    }
}

impl RedisClass {
    pub fn store_ws_data<T>(&self, exchange_name: &str, data: T) -> Result<(), RedisError>
    where
        T: Serialize,
    {
        let timestamp = Utc::now().timestamp();
        let key_ = format!("{}_{}", exchange_name, timestamp);

        // Serialize the data to a JSON string
        let json_str = match serde_json::to_string(&data) {
            Ok(json) => json,
            Err(json_err) => {
                return Err(RedisError::from((
                    redis::ErrorKind::IoError,
                    "Failed to serialize data to JSON",
                    json_err.to_string(),
                )))
            }
        };

        // Now, you have your JSON data in the desired Rust type (T), and you can store it in Redis
        let mut connection = match self.create_connection() {
            Ok(conn) => conn,
            Err(err) => {
                return Err(RedisError::from((
                    redis::ErrorKind::IoError,
                    "Failed to create Redis connection",
                    err.to_string(),
                )))
            }
        };

        // Store the JSON data in Redis as a JSON string
        if let Err(redis_err) =
            connection.hset::<&str, &str, &str, String>(key_.as_str(), key_.as_str(), &json_str)
        {
            return Err(RedisError::from((
                redis::ErrorKind::IoError,
                "Failed to store data in Redis",
                redis_err.to_string(),
            )));
        }

        Ok(())
    }
}

//TODO:
// Figure out a way to be able to get a hold of multiple dynamic keys being inserted in the db - Done
// Figure out a way to get all data for all keys and loop through it - Done
// Figure out how to group all data (data from each hash that we get from Redis) and output it as json - Done
// Build the API
// Find a way to pass the data to the frontend
