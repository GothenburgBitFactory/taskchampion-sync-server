//! This crate implements a Postgres storage backend for the TaskChampion sync server.
//!
//! Use the [`PostgresStorage`] type as an implementation of the [`Storage`] trait.
//!
//! This implementation is tested with Postgres version 17 but should work with any recent version.
//!
//! ## Schema Setup
//!
//! The database identified by the connection string must already exist and be set up with the
//! following schema (also available in `postgres/schema.sql` in the repository):
//!
//! ```sql
#![doc=include_str!("../schema.sql")]
//! ```
//!
//! ## Integration with External Applications
//!
//! The schema is stable, and any changes to the schema will be made in a major version with
//! migration instructions provided.
//!
//! An external application may:
//!  - Add additional tables to the database
//!  - Add additional columns to the `clients` table. If those columns do not have default
//!    values, calls to [`Txn::new_client`] will fail. It is possible to configure
//!    `taskchampion-sync-server` to never call this method.
//!  - Insert rows into the `clients` table, using default values for all columns except
//!    `client_id` and application-specific columns.
//!  - Delete rows from the `clients` table, using `CASCADE` to ensure any associated data
//!    is also deleted.

use anyhow::Context;
use bb8::PooledConnection;
use bb8_postgres::PostgresConnectionManager;
use chrono::{TimeZone, Utc};
use postgres_native_tls::MakeTlsConnector;
use taskchampion_sync_server_core::{Client, Snapshot, Storage, StorageTxn, Version};
use uuid::Uuid;

#[cfg(test)]
mod testing;

/// A storage backend which uses Postgres.
pub struct PostgresStorage {
    pool: bb8::Pool<PostgresConnectionManager<MakeTlsConnector>>,
}

impl PostgresStorage {
    pub async fn new(connection_string: impl ToString) -> anyhow::Result<Self> {
        let connector = native_tls::TlsConnector::new()?;
        let connector = postgres_native_tls::MakeTlsConnector::new(connector);
        let manager = PostgresConnectionManager::new_from_stringlike(connection_string, connector)?;
        let pool = bb8::Pool::builder().build(manager).await?;
        Ok(Self { pool })
    }
}

#[async_trait::async_trait]
impl Storage for PostgresStorage {
    async fn txn(&self, client_id: Uuid) -> anyhow::Result<Box<dyn StorageTxn + '_>> {
        let db_client = self.pool.get_owned().await?;

        db_client
            .execute("BEGIN TRANSACTION ISOLATION LEVEL SERIALIZABLE", &[])
            .await?;

        Ok(Box::new(Txn {
            client_id,
            db_client: Some(db_client),
        }))
    }
}

struct Txn {
    client_id: Uuid,
    /// The DB client or, if `commit` has been called, None. This ensures queries aren't executed
    /// after commit, and also frees connections back to the pool as quickly as possible.
    db_client: Option<PooledConnection<'static, PostgresConnectionManager<MakeTlsConnector>>>,
}

impl Txn {
    /// Get the db_client, or panic if it is gone (after commit).
    fn db_client(&self) -> &tokio_postgres::Client {
        let Some(db_client) = &self.db_client else {
            panic!("Cannot use a postgres Txn after commit");
        };
        db_client
    }

    /// Implementation for queries from the versions table
    async fn get_version_impl(
        &mut self,
        query: &'static str,
        client_id: Uuid,
        version_id_arg: Uuid,
    ) -> anyhow::Result<Option<Version>> {
        Ok(self
            .db_client()
            .query_opt(query, &[&version_id_arg, &client_id])
            .await
            .context("error getting version")?
            .map(|r| Version {
                version_id: r.get(0),
                parent_version_id: r.get(1),
                history_segment: r.get("history_segment"),
            }))
    }
}

#[async_trait::async_trait(?Send)]
impl StorageTxn for Txn {
    async fn get_client(&mut self) -> anyhow::Result<Option<Client>> {
        Ok(self
            .db_client()
            .query_opt(
                "SELECT
                    latest_version_id,
                    snapshot_timestamp,
                    versions_since_snapshot,
                    snapshot_version_id
                 FROM clients
                 WHERE client_id = $1
                 LIMIT 1",
                &[&self.client_id],
            )
            .await
            .context("error getting client")?
            .map(|r| {
                let latest_version_id: Uuid = r.get(0);
                let snapshot_timestamp: Option<i64> = r.get(1);
                let versions_since_snapshot: Option<i32> = r.get(2);
                let snapshot_version_id: Option<Uuid> = r.get(3);

                // if all of the relevant fields are non-NULL, return a snapshot
                let snapshot = match (
                    snapshot_timestamp,
                    versions_since_snapshot,
                    snapshot_version_id,
                ) {
                    (Some(ts), Some(vs), Some(v)) => Some(Snapshot {
                        version_id: v,
                        timestamp: Utc.timestamp_opt(ts, 0).unwrap(),
                        versions_since: vs as u32,
                    }),
                    _ => None,
                };
                Client {
                    latest_version_id,
                    snapshot,
                }
            }))
    }

    async fn new_client(&mut self, latest_version_id: Uuid) -> anyhow::Result<()> {
        self.db_client()
            .execute(
                "INSERT INTO clients (client_id, latest_version_id) VALUES ($1, $2)",
                &[&self.client_id, &latest_version_id],
            )
            .await
            .context("error creating/updating client")?;
        Ok(())
    }

    async fn set_snapshot(&mut self, snapshot: Snapshot, data: Vec<u8>) -> anyhow::Result<()> {
        let timestamp = snapshot.timestamp.timestamp();
        self.db_client()
            .execute(
                "UPDATE clients
                    SET snapshot_version_id = $1,
                        versions_since_snapshot = $2,
                        snapshot_timestamp = $3,
                        snapshot = $4
                    WHERE client_id = $5",
                &[
                    &snapshot.version_id,
                    &(snapshot.versions_since as i32),
                    &timestamp,
                    &data,
                    &self.client_id,
                ],
            )
            .await
            .context("error setting snapshot")?;
        Ok(())
    }

    async fn get_snapshot_data(&mut self, version_id: Uuid) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self
            .db_client()
            .query_opt(
                "SELECT snapshot
                 FROM clients
                 WHERE client_id = $1 and snapshot_version_id = $2
                 LIMIT 1",
                &[&self.client_id, &version_id],
            )
            .await
            .context("error getting snapshot data")?
            .map(|r| r.get(0)))
    }

    async fn get_version_by_parent(
        &mut self,
        parent_version_id: Uuid,
    ) -> anyhow::Result<Option<Version>> {
        self.get_version_impl(
            "SELECT version_id, parent_version_id, history_segment
                FROM versions
                WHERE parent_version_id = $1 AND client_id = $2",
            self.client_id,
            parent_version_id,
        )
        .await
    }

    async fn get_version(&mut self, version_id: Uuid) -> anyhow::Result<Option<Version>> {
        self.get_version_impl(
            "SELECT version_id, parent_version_id, history_segment
                FROM versions
                WHERE version_id = $1 AND client_id = $2",
            self.client_id,
            version_id,
        )
        .await
    }

    async fn add_version(
        &mut self,
        version_id: Uuid,
        parent_version_id: Uuid,
        history_segment: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.db_client()
            .execute(
                "INSERT INTO versions (version_id, client_id, parent_version_id, history_segment)
                VALUES ($1, $2, $3, $4)",
                &[
                    &version_id,
                    &self.client_id,
                    &parent_version_id,
                    &history_segment,
                ],
            )
            .await
            .context("error inserting new version")?;
        let rows_modified = self
            .db_client()
            .execute(
                "UPDATE clients
                    SET latest_version_id = $1,
                        versions_since_snapshot = versions_since_snapshot + 1
                    WHERE client_id = $2 and latest_version_id = $3",
                &[&version_id, &self.client_id, &parent_version_id],
            )
            .await
            .context("error updating latest_version_id")?;

        // If no rows were modified, this operation failed.
        if rows_modified == 0 {
            anyhow::bail!("clients.latest_version_id does not match parent_version_id");
        }
        Ok(())
    }

    async fn commit(&mut self) -> anyhow::Result<()> {
        self.db_client().execute("COMMIT", &[]).await?;
        self.db_client = None;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::testing::with_db;

    async fn make_client(db_client: &tokio_postgres::Client) -> anyhow::Result<Uuid> {
        let client_id = Uuid::new_v4();
        db_client
            .execute("insert into clients (client_id) values ($1)", &[&client_id])
            .await?;
        Ok(client_id)
    }

    async fn make_version(
        db_client: &tokio_postgres::Client,
        client_id: Uuid,
        parent_version_id: Uuid,
        history_segment: &[u8],
    ) -> anyhow::Result<Uuid> {
        let version_id = Uuid::new_v4();
        db_client
            .execute(
                "insert into versions
                    (version_id, client_id, parent_version_id, history_segment)
                    values ($1, $2, $3, $4)",
                &[
                    &version_id,
                    &client_id,
                    &parent_version_id,
                    &history_segment,
                ],
            )
            .await?;
        Ok(version_id)
    }

    async fn set_client_latest_version_id(
        db_client: &tokio_postgres::Client,
        client_id: Uuid,
        latest_version_id: Uuid,
    ) -> anyhow::Result<()> {
        db_client
            .execute(
                "update clients set latest_version_id = $1 where client_id = $2",
                &[&latest_version_id, &client_id],
            )
            .await?;
        Ok(())
    }

    async fn set_client_snapshot(
        db_client: &tokio_postgres::Client,
        client_id: Uuid,
        snapshot_version_id: Uuid,
        versions_since_snapshot: u32,
        snapshot_timestamp: i64,
        snapshot: &[u8],
    ) -> anyhow::Result<()> {
        db_client
            .execute(
                "
                update clients
                    set snapshot_version_id = $1,
                        versions_since_snapshot = $2,
                        snapshot_timestamp = $3,
                        snapshot = $4
                    where client_id = $5",
                &[
                    &snapshot_version_id,
                    &(versions_since_snapshot as i32),
                    &snapshot_timestamp,
                    &snapshot,
                    &client_id,
                ],
            )
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_get_client_none() -> anyhow::Result<()> {
        with_db(async |connection_string, _db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = Uuid::new_v4();
            let mut txn = storage.txn(client_id).await?;
            assert_eq!(txn.get_client().await?, None);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn test_get_client_exists_empty() -> anyhow::Result<()> {
        with_db(async |connection_string, db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = make_client(&db_client).await?;
            let mut txn = storage.txn(client_id).await?;
            assert_eq!(
                txn.get_client().await?,
                Some(Client {
                    latest_version_id: Uuid::nil(),
                    snapshot: None
                })
            );
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn test_get_client_exists_latest() -> anyhow::Result<()> {
        with_db(async |connection_string, db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = make_client(&db_client).await?;
            let latest_version_id = Uuid::new_v4();
            set_client_latest_version_id(&db_client, client_id, latest_version_id).await?;
            let mut txn = storage.txn(client_id).await?;
            assert_eq!(
                txn.get_client().await?,
                Some(Client {
                    latest_version_id,
                    snapshot: None
                })
            );
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn test_get_client_exists_with_snapshot() -> anyhow::Result<()> {
        with_db(async |connection_string, db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = make_client(&db_client).await?;
            let snapshot_version_id = Uuid::new_v4();
            let versions_since_snapshot = 10;
            let snapshot_timestamp = 10000000;
            let snapshot = b"abcd";
            set_client_snapshot(
                &db_client,
                client_id,
                snapshot_version_id,
                versions_since_snapshot,
                snapshot_timestamp,
                snapshot,
            )
            .await?;
            let mut txn = storage.txn(client_id).await?;
            assert_eq!(
                txn.get_client().await?,
                Some(Client {
                    latest_version_id: Uuid::nil(),
                    snapshot: Some(Snapshot {
                        version_id: snapshot_version_id,
                        timestamp: Utc.timestamp_opt(snapshot_timestamp, 0).unwrap(),
                        versions_since: versions_since_snapshot,
                    })
                })
            );
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn test_new_client() -> anyhow::Result<()> {
        with_db(async |connection_string, _db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = Uuid::new_v4();
            let latest_version_id = Uuid::new_v4();

            let mut txn1 = storage.txn(client_id).await?;
            txn1.new_client(latest_version_id).await?;

            // Client is not visible yet as txn1 is not committed.
            let mut txn2 = storage.txn(client_id).await?;
            assert_eq!(txn2.get_client().await?, None);

            txn1.commit().await?;

            // Client is now visible.
            let mut txn2 = storage.txn(client_id).await?;
            assert_eq!(
                txn2.get_client().await?,
                Some(Client {
                    latest_version_id,
                    snapshot: None
                })
            );

            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn test_set_snapshot() -> anyhow::Result<()> {
        with_db(async |connection_string, db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = make_client(&db_client).await?;
            let mut txn = storage.txn(client_id).await?;
            let snapshot_version_id = Uuid::new_v4();
            let versions_since_snapshot = 10;
            let snapshot_timestamp = 10000000;
            let snapshot = b"abcd";

            txn.set_snapshot(
                Snapshot {
                    version_id: snapshot_version_id,
                    timestamp: Utc.timestamp_opt(snapshot_timestamp, 0).unwrap(),
                    versions_since: versions_since_snapshot,
                },
                snapshot.to_vec(),
            )
            .await?;
            txn.commit().await?;

            txn = storage.txn(client_id).await?;
            assert_eq!(
                txn.get_client().await?,
                Some(Client {
                    latest_version_id: Uuid::nil(),
                    snapshot: Some(Snapshot {
                        version_id: snapshot_version_id,
                        timestamp: Utc.timestamp_opt(snapshot_timestamp, 0).unwrap(),
                        versions_since: versions_since_snapshot,
                    })
                })
            );

            let row = db_client
                .query_one(
                    "select snapshot from clients where client_id = $1",
                    &[&client_id],
                )
                .await?;
            assert_eq!(row.get::<_, &[u8]>(0), b"abcd");

            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn test_get_snapshot_none() -> anyhow::Result<()> {
        with_db(async |connection_string, db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = make_client(&db_client).await?;
            let mut txn = storage.txn(client_id).await?;
            assert_eq!(txn.get_snapshot_data(Uuid::new_v4()).await?, None);

            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn test_get_snapshot_mismatched_version() -> anyhow::Result<()> {
        with_db(async |connection_string, db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = make_client(&db_client).await?;
            let mut txn = storage.txn(client_id).await?;

            let snapshot_version_id = Uuid::new_v4();
            let versions_since_snapshot = 10;
            let snapshot_timestamp = 10000000;
            let snapshot = b"abcd";
            txn.set_snapshot(
                Snapshot {
                    version_id: snapshot_version_id,
                    timestamp: Utc.timestamp_opt(snapshot_timestamp, 0).unwrap(),
                    versions_since: versions_since_snapshot,
                },
                snapshot.to_vec(),
            )
            .await?;

            assert_eq!(txn.get_snapshot_data(Uuid::new_v4()).await?, None);

            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn test_get_version() -> anyhow::Result<()> {
        with_db(async |connection_string, db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = make_client(&db_client).await?;
            let parent_version_id = Uuid::new_v4();
            let version_id = make_version(&db_client, client_id, parent_version_id, b"v1").await?;

            let mut txn = storage.txn(client_id).await?;

            // Different parent doesn't exist.
            assert_eq!(txn.get_version_by_parent(Uuid::new_v4()).await?, None);

            // Different version doesn't exist.
            assert_eq!(txn.get_version(Uuid::new_v4()).await?, None);

            let version = Version {
                version_id,
                parent_version_id,
                history_segment: b"v1".to_vec(),
            };

            // Version found by parent.
            assert_eq!(
                txn.get_version_by_parent(parent_version_id).await?,
                Some(version.clone())
            );

            // Version found by ID.
            assert_eq!(txn.get_version(version_id).await?, Some(version));

            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn test_add_version() -> anyhow::Result<()> {
        with_db(async |connection_string, db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = make_client(&db_client).await?;
            let mut txn = storage.txn(client_id).await?;
            let version_id = Uuid::new_v4();
            txn.add_version(version_id, Uuid::nil(), b"v1".to_vec())
                .await?;
            assert_eq!(
                txn.get_version(version_id).await?,
                Some(Version {
                    version_id,
                    parent_version_id: Uuid::nil(),
                    history_segment: b"v1".to_vec()
                })
            );
            Ok(())
        })
        .await
    }

    #[tokio::test]
    /// When an add_version call specifies an incorrect `parent_version_id, it fails. This is
    /// typically avoided by calling `get_client` beforehand, which (due to repeatable reads)
    /// allows the caller to check the `latest_version_id` before calling `add_version`.
    async fn test_add_version_mismatch() -> anyhow::Result<()> {
        with_db(async |connection_string, db_client| {
            let storage = PostgresStorage::new(connection_string).await?;
            let client_id = make_client(&db_client).await?;
            let latest_version_id = Uuid::new_v4();
            set_client_latest_version_id(&db_client, client_id, latest_version_id).await?;

            let mut txn = storage.txn(client_id).await?;
            let version_id = Uuid::new_v4();
            let parent_version_id = Uuid::new_v4(); // != latest_version_id
            let res = txn
                .add_version(version_id, parent_version_id, b"v1".to_vec())
                .await;
            assert!(res.is_err());
            Ok(())
        })
        .await
    }

    #[tokio::test]
    /// Adding versions to two different clients can proceed concurrently.
    async fn test_add_version_no_conflict_different_clients() -> anyhow::Result<()> {
        with_db(async |connection_string, db_client| {
            let storage = PostgresStorage::new(connection_string).await?;

            // Clients 1 and 2 do not interfere with each other; if these are the same client, then
            // this will deadlock as one transaction waits for the other.
            let client_id1 = make_client(&db_client).await?;
            let mut txn1 = storage.txn(client_id1).await?;
            let version_id1 = Uuid::new_v4();
            txn1.add_version(version_id1, Uuid::nil(), b"v1".to_vec())
                .await?;
            assert_eq!(
                txn1.get_version(version_id1).await?,
                Some(Version {
                    version_id: version_id1,
                    parent_version_id: Uuid::nil(),
                    history_segment: b"v1".to_vec()
                })
            );

            let client_id2 = make_client(&db_client).await?;
            let mut txn2 = storage.txn(client_id2).await?;
            let version_id2 = Uuid::new_v4();
            txn2.add_version(version_id2, Uuid::nil(), b"v2".to_vec())
                .await?;
            assert_eq!(
                txn2.get_version(version_id2).await?,
                Some(Version {
                    version_id: version_id2,
                    parent_version_id: Uuid::nil(),
                    history_segment: b"v2".to_vec()
                })
            );

            txn1.commit().await?;
            txn2.commit().await?;

            Ok(())
        })
        .await
    }
}
