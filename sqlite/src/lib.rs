//! Tihs crate implements a SQLite storage backend for the TaskChampion sync server.
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
    /// directory.
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

impl Storage for SqliteStorage {
    fn txn<'a>(&'a self) -> anyhow::Result<Box<dyn StorageTxn + 'a>> {
        let con = self.new_connection()?;
        // Begin the transaction on this new connection. An IMMEDIATE connection is in
        // write (exclusive) mode from the start.
        con.execute("BEGIN IMMEDIATE", [])?;
        let txn = Txn { con };
        Ok(Box::new(txn))
    }
}

struct Txn {
    // SQLite only allows one concurrent transaction per connection, and rusqlite emulates
    // transactions by running `BEGIN ...` and `COMMIT` at appropriate times. So we will do
    // the same.
    con: Connection,
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

impl StorageTxn for Txn {
    fn get_client(&mut self, client_id: Uuid) -> anyhow::Result<Option<Client>> {
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
                [&StoredUuid(client_id)],
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

    fn new_client(&mut self, client_id: Uuid, latest_version_id: Uuid) -> anyhow::Result<()> {
        self.con
            .execute(
                "INSERT OR REPLACE INTO clients (client_id, latest_version_id) VALUES (?, ?)",
                params![&StoredUuid(client_id), &StoredUuid(latest_version_id)],
            )
            .context("Error creating/updating client")?;
        Ok(())
    }

    fn set_snapshot(
        &mut self,
        client_id: Uuid,
        snapshot: Snapshot,
        data: Vec<u8>,
    ) -> anyhow::Result<()> {
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
                    &StoredUuid(client_id),
                ],
            )
            .context("Error creating/updating snapshot")?;
        Ok(())
    }

    fn get_snapshot_data(
        &mut self,
        client_id: Uuid,
        version_id: Uuid,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let r = self
            .con
            .query_row(
                "SELECT snapshot, snapshot_version_id FROM clients WHERE client_id = ?",
                params![&StoredUuid(client_id)],
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

    fn get_version_by_parent(
        &mut self,
        client_id: Uuid,
        parent_version_id: Uuid,
    ) -> anyhow::Result<Option<Version>> {
        self.get_version_impl(
            "SELECT version_id, parent_version_id, history_segment FROM versions WHERE parent_version_id = ? AND client_id = ?",
            client_id,
            parent_version_id)
    }

    fn get_version(
        &mut self,
        client_id: Uuid,
        version_id: Uuid,
    ) -> anyhow::Result<Option<Version>> {
        self.get_version_impl(
            "SELECT version_id, parent_version_id, history_segment FROM versions WHERE version_id = ? AND client_id = ?",
            client_id,
            version_id)
    }

    fn add_version(
        &mut self,
        client_id: Uuid,
        version_id: Uuid,
        parent_version_id: Uuid,
        history_segment: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.con.execute(
            "INSERT INTO versions (version_id, client_id, parent_version_id, history_segment) VALUES(?, ?, ?, ?)",
            params![
                StoredUuid(version_id),
                StoredUuid(client_id),
                StoredUuid(parent_version_id),
                history_segment
            ]
        )
        .context("Error adding version")?;
        self.con
            .execute(
                "UPDATE clients
             SET
               latest_version_id = ?,
               versions_since_snapshot = versions_since_snapshot + 1
             WHERE client_id = ?",
                params![StoredUuid(version_id), StoredUuid(client_id),],
            )
            .context("Error updating client for new version")?;

        Ok(())
    }

    fn commit(&mut self) -> anyhow::Result<()> {
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

    #[test]
    fn test_emtpy_dir() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let non_existant = tmp_dir.path().join("subdir");
        let storage = SqliteStorage::new(non_existant)?;
        let mut txn = storage.txn()?;
        let maybe_client = txn.get_client(Uuid::new_v4())?;
        assert!(maybe_client.is_none());
        Ok(())
    }

    #[test]
    fn test_get_client_empty() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let mut txn = storage.txn()?;
        let maybe_client = txn.get_client(Uuid::new_v4())?;
        assert!(maybe_client.is_none());
        Ok(())
    }

    #[test]
    fn test_client_storage() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let mut txn = storage.txn()?;

        let client_id = Uuid::new_v4();
        let latest_version_id = Uuid::new_v4();
        txn.new_client(client_id, latest_version_id)?;

        let client = txn.get_client(client_id)?.unwrap();
        assert_eq!(client.latest_version_id, latest_version_id);
        assert!(client.snapshot.is_none());

        let latest_version_id = Uuid::new_v4();
        txn.add_version(client_id, latest_version_id, Uuid::new_v4(), vec![1, 1])?;

        let client = txn.get_client(client_id)?.unwrap();
        assert_eq!(client.latest_version_id, latest_version_id);
        assert!(client.snapshot.is_none());

        let snap = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: "2014-11-28T12:00:09Z".parse::<DateTime<Utc>>().unwrap(),
            versions_since: 4,
        };
        txn.set_snapshot(client_id, snap.clone(), vec![1, 2, 3])?;

        let client = txn.get_client(client_id)?.unwrap();
        assert_eq!(client.latest_version_id, latest_version_id);
        assert_eq!(client.snapshot.unwrap(), snap);

        Ok(())
    }

    #[test]
    fn test_gvbp_empty() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let mut txn = storage.txn()?;
        let maybe_version = txn.get_version_by_parent(Uuid::new_v4(), Uuid::new_v4())?;
        assert!(maybe_version.is_none());
        Ok(())
    }

    #[test]
    fn test_add_version_and_get_version() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let mut txn = storage.txn()?;

        let client_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let history_segment = b"abc".to_vec();
        txn.add_version(
            client_id,
            version_id,
            parent_version_id,
            history_segment.clone(),
        )?;

        let expected = Version {
            version_id,
            parent_version_id,
            history_segment,
        };

        let version = txn
            .get_version_by_parent(client_id, parent_version_id)?
            .unwrap();
        assert_eq!(version, expected);

        let version = txn.get_version(client_id, version_id)?.unwrap();
        assert_eq!(version, expected);

        Ok(())
    }

    #[test]
    fn test_snapshots() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let storage = SqliteStorage::new(tmp_dir.path())?;
        let mut txn = storage.txn()?;

        let client_id = Uuid::new_v4();

        txn.new_client(client_id, Uuid::new_v4())?;
        assert!(txn.get_client(client_id)?.unwrap().snapshot.is_none());

        let snap = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: "2013-10-08T12:00:09Z".parse::<DateTime<Utc>>().unwrap(),
            versions_since: 3,
        };
        txn.set_snapshot(client_id, snap.clone(), vec![9, 8, 9])?;

        assert_eq!(
            txn.get_snapshot_data(client_id, snap.version_id)?.unwrap(),
            vec![9, 8, 9]
        );
        assert_eq!(txn.get_client(client_id)?.unwrap().snapshot, Some(snap));

        let snap2 = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: "2014-11-28T12:00:09Z".parse::<DateTime<Utc>>().unwrap(),
            versions_since: 10,
        };
        txn.set_snapshot(client_id, snap2.clone(), vec![0, 2, 4, 6])?;

        assert_eq!(
            txn.get_snapshot_data(client_id, snap2.version_id)?.unwrap(),
            vec![0, 2, 4, 6]
        );
        assert_eq!(txn.get_client(client_id)?.unwrap().snapshot, Some(snap2));

        // check that mismatched version is detected
        assert!(txn.get_snapshot_data(client_id, Uuid::new_v4()).is_err());

        Ok(())
    }
}
