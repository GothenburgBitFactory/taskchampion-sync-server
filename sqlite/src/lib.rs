//! This crate implements a SQLite storage backend for the TaskChampion sync server.
//!
//! Use the [`SqliteStorage`] type as an implementation of the [`Storage`] trait.
//!
//! This crate is intended for small deployments of a sync server, supporting one or a small number
//! of users. The schema for the database is considered an implementation detail. For more robust
//! database support, consider `taskchampion-sync-server-storage-postgres`.

use anyhow::Context;
use chrono::{TimeZone, Utc};
use rusqlite::types::{FromSql, ToSql};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use taskchampion_sync_server_core::{Client, Snapshot, Storage, StorageTxn, Version};
use uuid::Uuid;

/// Newtype to allow implementing `FromSql` for foreign `uuid::Uuid`
struct StoredUuid(Uuid);

/// Conversion from Uuid stored as a string (rusqlite's uuid feature stores as binary blob)
impl FromSql for StoredUuid {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let u = Uuid::parse_str(value.as_str()?)
            .map_err(|_| rusqlite::types::FromSqlError::InvalidType)?;
        Ok(StoredUuid(u))
    }
}

/// Store Uuid as string in database
impl ToSql for StoredUuid {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        let s = self.0.to_string();
        Ok(s.into())
    }
}

/// An on-disk storage backend which uses SQLite.
///
/// A new connection is opened for each transaction, and only one transaction may be active at a
/// time; a second call to `txn` will block until the first transaction is dropped.
pub struct SqliteStorage {
    db_file: std::path::PathBuf,
}

impl SqliteStorage {
    fn new_connection(&self) -> anyhow::Result<Connection> {
        Ok(Connection::open(&self.db_file)?)
    }

    /// Create a new instance using a database at the given directory.
    ///
    /// The database will be stored in a file named `taskchampion-sync-server.sqlite3` in the given
    /// directory. The database will be created if it does not exist.
    pub fn new<P: AsRef<Path>>(directory: P) -> anyhow::Result<SqliteStorage> {
        std::fs::create_dir_all(&directory)
            .with_context(|| format!("Failed to create `{}`.", directory.as_ref().display()))?;
        let db_file = directory.as_ref().join("taskchampion-sync-server.sqlite3");

        let o = SqliteStorage { db_file };

        let con = o.new_connection()?;

        // Use the modern WAL mode.
        con.query_row("PRAGMA journal_mode=WAL", [], |_row| Ok(()))
            .context("Setting journal_mode=WAL")?;

        let queries = vec![
                "CREATE TABLE IF NOT EXISTS clients (
                    client_id STRING PRIMARY KEY,
                    latest_version_id STRING,
                    snapshot_version_id STRING,
                    versions_since_snapshot INTEGER,
                    snapshot_timestamp INTEGER,
                    snapshot BLOB);",
                "CREATE TABLE IF NOT EXISTS versions (version_id STRING PRIMARY KEY, client_id STRING, parent_version_id STRING, history_segment BLOB);",
                "CREATE INDEX IF NOT EXISTS versions_by_parent ON versions (parent_version_id);",
            ];
        for q in queries {
            con.execute(q, [])
                .context("Error while creating SQLite tables")?;
        }

        Ok(o)
    }
}

#[async_trait::async_trait]
impl Storage for SqliteStorage {
    async fn txn(&self, client_id: Uuid) -> anyhow::Result<Box<dyn StorageTxn + '_>> {
        let con = self.new_connection()?;
        // Begin the transaction on this new connection. An IMMEDIATE connection is in
        // write (exclusive) mode from the start.
        con.execute("BEGIN IMMEDIATE", [])?;
        let txn = Txn { con, client_id };
        Ok(Box::new(txn))
    }
}

struct Txn {
    // SQLite only allows one concurrent transaction per connection, and rusqlite emulates
    // transactions by running `BEGIN ...` and `COMMIT` at appropriate times. So we will do
    // the same.
    con: Connection,
    client_id: Uuid,
}

impl Txn {
    /// Implementation for queries from the versions table
    fn get_version_impl(
        &mut self,
        query: &'static str,
        client_id: Uuid,
        version_id_arg: Uuid,
    ) -> anyhow::Result<Option<Version>> {
        let r = self
            .con
            .query_row(
                query,
                params![&StoredUuid(version_id_arg), &StoredUuid(client_id)],
                |r| {
                    let version_id: StoredUuid = r.get("version_id")?;
                    let parent_version_id: StoredUuid = r.get("parent_version_id")?;

                    Ok(Version {
                        version_id: version_id.0,
                        parent_version_id: parent_version_id.0,
                        history_segment: r.get("history_segment")?,
                    })
                },
            )
            .optional()
            .context("Error getting version")?;
        Ok(r)
    }
}

#[async_trait::async_trait(?Send)]
impl StorageTxn for Txn {
    async fn get_client(&mut self) -> anyhow::Result<Option<Client>> {
        let result: Option<Client> = self
            .con
            .query_row(
                "SELECT
                    latest_version_id,
                    snapshot_timestamp,
                    versions_since_snapshot,
                    snapshot_version_id
                 FROM clients
                 WHERE client_id = ?
                 LIMIT 1",
                [&StoredUuid(self.client_id)],
                |r| {
                    let latest_version_id: StoredUuid = r.get(0)?;
                    let snapshot_timestamp: Option<i64> = r.get(1)?;
                    let versions_since_snapshot: Option<u32> = r.get(2)?;
                    let snapshot_version_id: Option<StoredUuid> = r.get(3)?;

                    // if all of the relevant fields are non-NULL, return a snapshot
                    let snapshot = match (
                        snapshot_timestamp,
                        versions_since_snapshot,
                        snapshot_version_id,
                    ) {
                        (Some(ts), Some(vs), Some(v)) => Some(Snapshot {
                            version_id: v.0,
                            timestamp: Utc.timestamp_opt(ts, 0).unwrap(),
                            versions_since: vs,
                        }),
                        _ => None,
                    };
                    Ok(Client {
                        latest_version_id: latest_version_id.0,
                        snapshot,
                    })
                },
            )
            .optional()
            .context("Error getting client")?;

        Ok(result)
    }

    async fn new_client(&mut self, latest_version_id: Uuid) -> anyhow::Result<()> {
        self.con
            .execute(
                "INSERT INTO clients (client_id, latest_version_id) VALUES (?, ?)",
                params![&StoredUuid(self.client_id), &StoredUuid(latest_version_id)],
            )
            .context("Error creating/updating client")?;
        Ok(())
    }

    async fn set_snapshot(&mut self, snapshot: Snapshot, data: Vec<u8>) -> anyhow::Result<()> {
        self.con
            .execute(
                "UPDATE clients
             SET
               snapshot_version_id = ?,
               snapshot_timestamp = ?,
               versions_since_snapshot = ?,
               snapshot = ?
             WHERE client_id = ?",
                params![
                    &StoredUuid(snapshot.version_id),
                    snapshot.timestamp.timestamp(),
                    snapshot.versions_since,
                    data,
                    &StoredUuid(self.client_id),
                ],
            )
            .context("Error creating/updating snapshot")?;
        Ok(())
    }

    async fn get_snapshot_data(&mut self, version_id: Uuid) -> anyhow::Result<Option<Vec<u8>>> {
        let r = self
            .con
            .query_row(
                "SELECT snapshot, snapshot_version_id FROM clients WHERE client_id = ?",
                params![&StoredUuid(self.client_id)],
                |r| {
                    let v: StoredUuid = r.get("snapshot_version_id")?;
                    let d: Vec<u8> = r.get("snapshot")?;
                    Ok((v.0, d))
                },
            )
            .optional()
            .context("Error getting snapshot")?;
        r.map(|(v, d)| {
            if v != version_id {
                return Err(anyhow::anyhow!("unexpected snapshot_version_id"));
            }

            Ok(d)
        })
        .transpose()
    }

    async fn get_version_by_parent(
        &mut self,
        parent_version_id: Uuid,
    ) -> anyhow::Result<Option<Version>> {
        self.get_version_impl(
            "SELECT version_id, parent_version_id, history_segment FROM versions WHERE parent_version_id = ? AND client_id = ?",
            self.client_id,
            parent_version_id)
    }

    async fn get_version(&mut self, version_id: Uuid) -> anyhow::Result<Option<Version>> {
        self.get_version_impl(
            "SELECT version_id, parent_version_id, history_segment FROM versions WHERE version_id = ? AND client_id = ?",
            self.client_id,
            version_id)
    }

    async fn add_version(
        &mut self,
        version_id: Uuid,
        parent_version_id: Uuid,
        history_segment: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.con.execute(
            "INSERT INTO versions (version_id, client_id, parent_version_id, history_segment) VALUES(?, ?, ?, ?)",
            params![
                StoredUuid(version_id),
                StoredUuid(self.client_id),
                StoredUuid(parent_version_id),
                history_segment
            ]
        )
        .context("Error adding version")?;
        let rows_changed = self
            .con
            .execute(
                "UPDATE clients
             SET
               latest_version_id = ?,
               versions_since_snapshot = versions_since_snapshot + 1
             WHERE client_id = ? and (latest_version_id = ? or latest_version_id = ?)",
                params![
                    StoredUuid(version_id),
                    StoredUuid(self.client_id),
                    StoredUuid(parent_version_id),
                    StoredUuid(Uuid::nil())
                ],
            )
            .context("Error updating client for new version")?;

        if rows_changed == 0 {
            anyhow::bail!("clients.latest_version_id does not match parent_version_id");
        }

        Ok(())
    }

    async fn commit(&mut self) -> anyhow::Result<()> {
        self.con.execute("COMMIT", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::DateTime;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_emtpy_dir() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let non_existant = tmp_dir.path().join("subdir");
        let storage = SqliteStorage::new(non_existant)?;
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;
        let maybe_client = txn.get_client().await?;
        assert!(maybe_client.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_client_empty() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;
        let maybe_client = txn.get_client().await?;
        assert!(maybe_client.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_client_storage() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;

        let latest_version_id = Uuid::new_v4();
        txn.new_client(latest_version_id).await?;

        let client = txn.get_client().await?.unwrap();
        assert_eq!(client.latest_version_id, latest_version_id);
        assert!(client.snapshot.is_none());

        let new_version_id = Uuid::new_v4();
        txn.add_version(new_version_id, latest_version_id, vec![1, 1])
            .await?;

        let client = txn.get_client().await?.unwrap();
        assert_eq!(client.latest_version_id, new_version_id);
        assert!(client.snapshot.is_none());

        let snap = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: "2014-11-28T12:00:09Z".parse::<DateTime<Utc>>().unwrap(),
            versions_since: 4,
        };
        txn.set_snapshot(snap.clone(), vec![1, 2, 3]).await?;

        let client = txn.get_client().await?.unwrap();
        assert_eq!(client.latest_version_id, new_version_id);
        assert_eq!(client.snapshot.unwrap(), snap);

        Ok(())
    }

    #[tokio::test]
    async fn test_gvbp_empty() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;
        let maybe_version = txn.get_version_by_parent(Uuid::new_v4()).await?;
        assert!(maybe_version.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_add_version_and_get_version() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;

        let parent_version_id = Uuid::new_v4();
        txn.new_client(parent_version_id).await?;

        let version_id = Uuid::new_v4();
        let history_segment = b"abc".to_vec();
        txn.add_version(version_id, parent_version_id, history_segment.clone())
            .await?;

        let expected = Version {
            version_id,
            parent_version_id,
            history_segment,
        };

        let version = txn.get_version_by_parent(parent_version_id).await?.unwrap();
        assert_eq!(version, expected);

        let version = txn.get_version(version_id).await?.unwrap();
        assert_eq!(version, expected);

        Ok(())
    }

    #[tokio::test]
    async fn test_add_version_exists() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;

        let parent_version_id = Uuid::new_v4();
        txn.new_client(parent_version_id).await?;

        let version_id = Uuid::new_v4();
        let history_segment = b"abc".to_vec();
        txn.add_version(version_id, parent_version_id, history_segment.clone())
            .await?;
        // Fails because the version already exists.
        assert!(txn
            .add_version(version_id, parent_version_id, history_segment.clone())
            .await
            .is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_add_version_mismatch() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;

        let latest_version_id = Uuid::new_v4();
        txn.new_client(latest_version_id).await?;

        let version_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4(); // != latest_version_id
        let history_segment = b"abc".to_vec();
        // Fails because the latest_version_id is not parent_version_id.
        assert!(txn
            .add_version(version_id, parent_version_id, history_segment.clone())
            .await
            .is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshots() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;

        txn.new_client(Uuid::new_v4()).await?;
        assert!(txn.get_client().await?.unwrap().snapshot.is_none());

        let snap = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: "2013-10-08T12:00:09Z".parse::<DateTime<Utc>>().unwrap(),
            versions_since: 3,
        };
        txn.set_snapshot(snap.clone(), vec![9, 8, 9]).await?;

        assert_eq!(
            txn.get_snapshot_data(snap.version_id).await?.unwrap(),
            vec![9, 8, 9]
        );
        assert_eq!(txn.get_client().await?.unwrap().snapshot, Some(snap));

        let snap2 = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: "2014-11-28T12:00:09Z".parse::<DateTime<Utc>>().unwrap(),
            versions_since: 10,
        };
        txn.set_snapshot(snap2.clone(), vec![0, 2, 4, 6]).await?;

        assert_eq!(
            txn.get_snapshot_data(snap2.version_id).await?.unwrap(),
            vec![0, 2, 4, 6]
        );
        assert_eq!(txn.get_client().await?.unwrap().snapshot, Some(snap2));

        // check that mismatched version is detected
        assert!(txn.get_snapshot_data(Uuid::new_v4()).await.is_err());

        Ok(())
    }

    #[tokio::test]
    /// When an add_version call specifies a `parent_version_id` that does not exist in the
    /// DB, but no other versions exist, the call succeeds.
    async fn test_add_version_no_history() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;
        txn.new_client(Uuid::nil()).await?;

        let version_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        txn.add_version(version_id, parent_version_id, b"v1".to_vec())
            .await?;
        Ok(())
    }
}
