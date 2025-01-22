// use redis::{AsyncCommands, Client};
// use crate::Result;

// /// This struct is used to interact with Redis for storing and retrieving
// /// entities of type `T`. It wraps the Redis client and provides methods
// /// to perform CRUD operations asynchronously.

// #[derive(Debug)]
// pub struct RedisClient {
//     client: Client,
// }

// impl RedisClient {
//     /// Creates a new `RedisRepository` instance.
//     ///
//     /// # Arguments
//     ///
//     /// * `redis_url` - A string slice that holds the URL of the Redis server.
//     ///
//     /// # Returns
//     ///
//     /// * `Ok(Self)` if the repository is successfully created.
//     /// * `Err(RedisError)` if there is an error opening the Redis connection.
//     ///
//     /// # Example
//     ///
//     /// ```rust
//     /// let repo = RedisRepository::new("redis://127.0.0.1/");
//     /// ```
//     pub fn new(redis_url: &str) -> Result<Self> {
//         let client = Client::open(redis_url)?;
//         match client {
//             Ok(client) => Ok(Self { client }),
//             Err(e) => Err(e.into()),
//         }
//     }

//         /// Creates a new entity in the Redis store.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - A string slice that represents the Redis key under which the entity will be stored.
//     /// * `entity` - A reference to the entity of type `T` to be stored.
//     ///
//     /// # Returns
//     ///
//     /// * `Ok(())` if the entity is successfully created.
//     /// * `Err(RepositoryError)` if there is an error during the operation.
//     ///
//     /// # Example
//     ///
//     /// ```rust
//     /// repo.create("test:key", &my_entity).await?;
//     /// ```
//     async fn create(&self, key: &str, entity: &T) -> Result<()> {
//         let mut conn = self.client.get_multiplexed_async_connection().await?;
//         let json = serde_json::to_string(entity)?;
//         () = conn.set(key, json).await?;
//         match () {
//             Ok(()) => Ok(()),
//             Err(e) => Err(e.into()),
//         }
//     }

//     /// Reads an entity from Redis by its key.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - A string slice that represents the Redis key of the entity to be retrieved.
//     ///
//     /// # Returns
//     ///
//     /// * `Ok(Some(entity))` if the entity exists and is successfully deserialized.
//     /// * `Ok(None)` if the entity does not exist.
//     /// * `Err(RepositoryError)` if there is an error during the operation.
//     ///
//     /// # Example
//     ///
//     /// ```rust
//     /// let entity = repo.read("test:key").await?;
//     /// ```
//     async fn read(&self, key: &str) -> Result<Option<T>, ClientError> {
//         let mut conn = self.client.get_multiplexed_async_connection().await?;
//         let json: Option<String> = conn.get(key).await?;
//         match json {
//             Some(data) => Ok(Some(serde_json::from_str(&data)?)),
//             None => Ok(None),
//         }
//     }

//     /// Updates an entity in Redis by creating or replacing it.
//     ///
//     /// This method uses the `create` method to update the entity.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - A string slice that represents the Redis key of the entity to be updated.
//     /// * `entity` - A reference to the entity of type `T` to be updated.
//     ///
//     /// # Returns
//     ///
//     /// * `Ok(())` if the entity is successfully updated.
//     /// * `Err(RepositoryError)` if there is an error during the operation.
//     ///
//     /// # Example
//     ///
//     /// ```rust
//     /// repo.update("test:key", &updated_entity).await?;
//     /// ```
//     async fn update(&self, key: &str, entity: &T) -> Result<(), ClientError> {
//         self.create(key, entity).await
//     }

//     /// Deletes an entity from Redis by its key.
//     ///
//     /// # Arguments
//     ///
//     /// * `key` - A string slice that represents the Redis key of the entity to be deleted.
//     ///
//     /// # Returns
//     ///
//     /// * `Ok(())` if the entity is successfully deleted.
//     /// * `Err(RepositoryError)` if there is an error during the operation.
//     ///
//     /// # Example
//     ///
//     /// ```rust
//     /// repo.delete("test:key").await?;
//     /// ```
//     async fn delete(&self, key: &str) -> Result<(), ClientError> {
//         let mut conn = self.client.get_multiplexed_async_connection().await?;
//         () = conn.del(key).await?;
//         Ok(())
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::sync::Arc;
//     use std::time::Duration;
//     use tokio::{self, time::timeout};

//     #[derive(Serialize, Deserialize, Debug, PartialEq)]
//     struct TestEntity {
//         name: String,
//         age: u32,
//     }

//     fn get_redis_url() -> String {
//         std::env::var("TEST_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string())
//     }

//     #[tokio::test]
//     async fn test_create_and_read() {
//         let redis_url = get_redis_url();
//         let repo: Arc<dyn Repository<TestEntity> + Send + Sync> = Arc::new(
//             RedisRepository::new(redis_url.as_str()).expect("Failed to create repository"),
//         );

//         let key = "test:create_and_read";
//         let entity = TestEntity {
//             name: "Alice".to_string(),
//             age: 30,
//         };

//         repo.create(key, &entity).await.expect("Create failed");

//         let retrieved = repo
//             .read(key)
//             .await
//             .expect("Read failed")
//             .expect("Entity not found");
//         assert_eq!(retrieved, entity);

//         repo.delete(key).await.expect("Delete failed");
//     }

//     #[tokio::test]
//     async fn test_update() {
//         let redis_url = get_redis_url();
//         let repo: Arc<dyn Repository<TestEntity> + Send + Sync> = Arc::new(
//             RedisRepository::new(redis_url.as_str()).expect("Failed to create repository"),
//         );

//         let key = "test:update";
//         let entity = TestEntity {
//             name: "Bob".to_string(),
//             age: 25,
//         };
//         let updated_entity = TestEntity {
//             name: "Bob".to_string(),
//             age: 26,
//         };
//         repo.create(key, &entity).await.expect("Create failed");

//         repo.update(key, &updated_entity)
//             .await
//             .expect("Update failed");

//         let retrieved = repo
//             .read(key)
//             .await
//             .expect("Read failed")
//             .expect("Entity not found");
//         assert_eq!(retrieved, updated_entity);

//         repo.delete(key).await.expect("Delete failed");
//     }

//     #[tokio::test]
//     async fn test_delete() {
//         let redis_url = get_redis_url();
//         let repo: Arc<dyn Repository<TestEntity> + Send + Sync> = Arc::new(
//             RedisRepository::new(redis_url.as_str()).expect("Failed to create repository"),
//         );

//         let key = "test:delete";
//         let entity = TestEntity {
//             name: "Charlie".to_string(),
//             age: 40,
//         };
//         repo.create(key, &entity).await.expect("Create failed");
//         repo.delete(key).await.expect("Delete failed");

//         let retrieved = repo.read(key).await.expect("Read failed");
//         assert!(retrieved.is_none());
//     }

//     #[tokio::test]
//     async fn test_connection_error() {
//         let invalid_redis_url = "redis://128.0.0.2/";
//         let repo: Arc<dyn Repository<TestEntity> + Send + Sync> =
//             Arc::new(RedisRepository::new(invalid_redis_url).expect("Failed to create repository"));

//         let key = "test:test_connection_error";
//         let entity = TestEntity {
//             name: "Alice".to_string(),
//             age: 30,
//         };
//         let create_result = timeout(Duration::from_secs(1), repo.create(key, &entity)).await;
//         assert!(
//             create_result.is_err(),
//             "Expected create to fail, but it succeeded"
//         );
//     }

//     #[tokio::test]
//     async fn test_create_with_invalid_data() {
//         let redis_url = get_redis_url();
//         let repo: Arc<dyn Repository<TestEntity> + Send + Sync> = Arc::new(
//             RedisRepository::new(redis_url.as_str()).expect("Failed to create repository"),
//         );
//         let key = "test:create_with_invalid_data";
//         let invalid_entity = TestEntity {
//             name: "InvalidEntity".to_string(),
//             age: 0,
//         };
//         let result = repo.create(key, &invalid_entity).await;
//         assert!(
//             result.is_ok(),
//             "Expected creation to fail, but it succeeded"
//         );
//         repo.delete(key).await.expect("Delete failed");
//     }

//     #[tokio::test]
//     async fn test_read_non_existing_key() {
//         let redis_url = get_redis_url();
//         let repo: Arc<dyn Repository<TestEntity> + Send + Sync> = Arc::new(
//             RedisRepository::new(redis_url.as_str()).expect("Failed to create repository"),
//         );

//         let key = "test:read_non_existing_key";
//         let result = repo.read(key).await.expect("Read failed");
//         assert!(result.is_none(), "Expected no data, but found data");
//     }

//     #[tokio::test]
//     async fn test_delete_non_existing_key() {
//         let redis_url = get_redis_url();
//         let repo: Arc<dyn Repository<TestEntity> + Send + Sync> = Arc::new(
//             RedisRepository::new(redis_url.as_str()).expect("Failed to create repository"),
//         );

//         let key = "test:delete_non_existing_key";

//         let result = repo.delete(key).await;
//         assert!(
//             result.is_ok(),
//             "Expected delete to succeed even for non-existing key"
//         );
//     }
// }
