use super::{Client, Snapshot, Storage, StorageTxn, Version};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

struct Inner {
    /// Clients, indexed by client_id
    clients: HashMap<Uuid, Client>,

    /// Snapshot data, indexed by client id
    snapshots: HashMap<Uuid, Vec<u8>>,

    /// Versions, indexed by (client_id, version_id)
    versions: HashMap<(Uuid, Uuid), Version>,

    /// Child versions, indexed by (client_id, parent_version_id)
    children: HashMap<(Uuid, Uuid), Uuid>,
}

/// In-memory storage for testing and experimentation.
///
/// This is not for production use, but supports testing of sync server implementations.
///
/// NOTE: this panics if changes were made in a transaction that is later dropped without being
/// committed, as this likely represents a bug that should be exposed in tests.
pub struct InMemoryStorage(Mutex<Inner>);

impl InMemoryStorage {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Mutex::new(Inner {
            clients: HashMap::new(),
            snapshots: HashMap::new(),
            versions: HashMap::new(),
            children: HashMap::new(),
        }))
    }
}

struct InnerTxn<'a> {
    client_id: Uuid,
    guard: MutexGuard<'a, Inner>,
    written: bool,
    committed: bool,
}

impl Storage for InMemoryStorage {
    fn txn(&self, client_id: Uuid) -> anyhow::Result<Box<dyn StorageTxn + '_>> {
        Ok(Box::new(InnerTxn {
            client_id,
            guard: self.0.lock().expect("poisoned lock"),
            written: false,
            committed: false,
        }))
    }
}

impl<'a> StorageTxn for InnerTxn<'a> {
    fn get_client(&mut self) -> anyhow::Result<Option<Client>> {
        Ok(self.guard.clients.get(&self.client_id).cloned())
    }

    fn new_client(&mut self, latest_version_id: Uuid) -> anyhow::Result<()> {
        if self.guard.clients.contains_key(&self.client_id) {
            return Err(anyhow::anyhow!("Client {} already exists", self.client_id));
        }
        self.guard.clients.insert(
            self.client_id,
            Client {
                latest_version_id,
                snapshot: None,
            },
        );
        self.written = true;
        Ok(())
    }

    fn set_snapshot(&mut self, snapshot: Snapshot, data: Vec<u8>) -> anyhow::Result<()> {
        let client = self
            .guard
            .clients
            .get_mut(&self.client_id)
            .ok_or_else(|| anyhow::anyhow!("no such client"))?;
        client.snapshot = Some(snapshot);
        self.guard.snapshots.insert(self.client_id, data);
        self.written = true;
        Ok(())
    }

    fn get_snapshot_data(&mut self, version_id: Uuid) -> anyhow::Result<Option<Vec<u8>>> {
        // sanity check
        let client = self.guard.clients.get(&self.client_id);
        let client = client.ok_or_else(|| anyhow::anyhow!("no such client"))?;
        if Some(&version_id) != client.snapshot.as_ref().map(|snap| &snap.version_id) {
            return Err(anyhow::anyhow!("unexpected snapshot_version_id"));
        }
        Ok(self.guard.snapshots.get(&self.client_id).cloned())
    }

    fn get_version_by_parent(
        &mut self,
        parent_version_id: Uuid,
    ) -> anyhow::Result<Option<Version>> {
        if let Some(parent_version_id) = self
            .guard
            .children
            .get(&(self.client_id, parent_version_id))
        {
            Ok(self
                .guard
                .versions
                .get(&(self.client_id, *parent_version_id))
                .cloned())
        } else {
            Ok(None)
        }
    }

    fn get_version(&mut self, version_id: Uuid) -> anyhow::Result<Option<Version>> {
        Ok(self
            .guard
            .versions
            .get(&(self.client_id, version_id))
            .cloned())
    }

    fn add_version(
        &mut self,
        version_id: Uuid,
        parent_version_id: Uuid,
        history_segment: Vec<u8>,
    ) -> anyhow::Result<()> {
        // TODO: verify it doesn't exist (`.entry`?)
        let version = Version {
            version_id,
            parent_version_id,
            history_segment,
        };

        if let Some(client) = self.guard.clients.get_mut(&self.client_id) {
            client.latest_version_id = version_id;
            if let Some(ref mut snap) = client.snapshot {
                snap.versions_since += 1;
            }
        } else {
            return Err(anyhow::anyhow!("Client {} does not exist", self.client_id));
        }

        self.guard
            .children
            .insert((self.client_id, parent_version_id), version_id);
        self.guard
            .versions
            .insert((self.client_id, version_id), version);

        self.written = true;
        Ok(())
    }

    fn commit(&mut self) -> anyhow::Result<()> {
        self.committed = true;
        Ok(())
    }
}

impl<'a> Drop for InnerTxn<'a> {
    fn drop(&mut self) {
        if self.written && !self.committed {
            panic!("Uncommitted InMemoryStorage transaction dropped without commiting");
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_get_client_empty() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let mut txn = storage.txn(Uuid::new_v4())?;
        let maybe_client = txn.get_client()?;
        assert!(maybe_client.is_none());
        Ok(())
    }

    #[test]
    fn test_client_storage() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id)?;

        let latest_version_id = Uuid::new_v4();
        txn.new_client(latest_version_id)?;

        let client = txn.get_client()?.unwrap();
        assert_eq!(client.latest_version_id, latest_version_id);
        assert!(client.snapshot.is_none());

        let latest_version_id = Uuid::new_v4();
        txn.add_version(latest_version_id, Uuid::new_v4(), vec![1, 1])?;

        let client = txn.get_client()?.unwrap();
        assert_eq!(client.latest_version_id, latest_version_id);
        assert!(client.snapshot.is_none());

        let snap = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            versions_since: 4,
        };
        txn.set_snapshot(snap.clone(), vec![1, 2, 3])?;

        let client = txn.get_client()?.unwrap();
        assert_eq!(client.latest_version_id, latest_version_id);
        assert_eq!(client.snapshot.unwrap(), snap);

        txn.commit()?;
        Ok(())
    }

    #[test]
    fn test_gvbp_empty() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id)?;
        let maybe_version = txn.get_version_by_parent(Uuid::new_v4())?;
        assert!(maybe_version.is_none());
        Ok(())
    }

    #[test]
    fn test_add_version_and_get_version() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id)?;

        let version_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let history_segment = b"abc".to_vec();

        txn.new_client(parent_version_id)?;
        txn.add_version(version_id, parent_version_id, history_segment.clone())?;

        let expected = Version {
            version_id,
            parent_version_id,
            history_segment,
        };

        let version = txn.get_version_by_parent(parent_version_id)?.unwrap();
        assert_eq!(version, expected);

        let version = txn.get_version(version_id)?.unwrap();
        assert_eq!(version, expected);

        txn.commit()?;
        Ok(())
    }

    #[test]
    fn test_snapshots() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id)?;

        txn.new_client(Uuid::new_v4())?;
        assert!(txn.get_client()?.unwrap().snapshot.is_none());

        let snap = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            versions_since: 3,
        };
        txn.set_snapshot(snap.clone(), vec![9, 8, 9])?;

        assert_eq!(
            txn.get_snapshot_data(snap.version_id)?.unwrap(),
            vec![9, 8, 9]
        );
        assert_eq!(txn.get_client()?.unwrap().snapshot, Some(snap));

        let snap2 = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            versions_since: 10,
        };
        txn.set_snapshot(snap2.clone(), vec![0, 2, 4, 6])?;

        assert_eq!(
            txn.get_snapshot_data(snap2.version_id)?.unwrap(),
            vec![0, 2, 4, 6]
        );
        assert_eq!(txn.get_client()?.unwrap().snapshot, Some(snap2));

        // check that mismatched version is detected
        assert!(txn.get_snapshot_data(Uuid::new_v4()).is_err());

        txn.commit()?;
        Ok(())
    }
}
