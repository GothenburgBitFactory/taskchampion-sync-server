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

#[async_trait::async_trait]
impl Storage for InMemoryStorage {
    async fn txn(&self, client_id: Uuid) -> anyhow::Result<Box<dyn StorageTxn + '_>> {
        Ok(Box::new(InnerTxn {
            client_id,
            guard: self.0.lock().expect("poisoned lock"),
            written: false,
            committed: false,
        }))
    }
}

#[async_trait::async_trait(?Send)]
impl StorageTxn for InnerTxn<'_> {
    async fn get_client(&mut self) -> anyhow::Result<Option<Client>> {
        Ok(self.guard.clients.get(&self.client_id).cloned())
    }

    async fn new_client(&mut self, latest_version_id: Uuid) -> anyhow::Result<()> {
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

    async fn set_snapshot(&mut self, snapshot: Snapshot, data: Vec<u8>) -> anyhow::Result<()> {
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

    async fn get_snapshot_data(&mut self, version_id: Uuid) -> anyhow::Result<Option<Vec<u8>>> {
        // sanity check
        let client = self.guard.clients.get(&self.client_id);
        let client = client.ok_or_else(|| anyhow::anyhow!("no such client"))?;
        if Some(&version_id) != client.snapshot.as_ref().map(|snap| &snap.version_id) {
            return Err(anyhow::anyhow!("unexpected snapshot_version_id"));
        }
        Ok(self.guard.snapshots.get(&self.client_id).cloned())
    }

    async fn get_version_by_parent(
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

    async fn get_version(&mut self, version_id: Uuid) -> anyhow::Result<Option<Version>> {
        Ok(self
            .guard
            .versions
            .get(&(self.client_id, version_id))
            .cloned())
    }

    async fn add_version(
        &mut self,
        version_id: Uuid,
        parent_version_id: Uuid,
        history_segment: Vec<u8>,
    ) -> anyhow::Result<()> {
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
            anyhow::bail!("Client {} does not exist", self.client_id);
        }

        if self
            .guard
            .children
            .insert((self.client_id, parent_version_id), version_id)
            .is_some()
        {
            anyhow::bail!(
                "Client {} already has a child for {}",
                self.client_id,
                parent_version_id
            );
        }
        if self
            .guard
            .versions
            .insert((self.client_id, version_id), version)
            .is_some()
        {
            anyhow::bail!(
                "Client {} already has a version {}",
                self.client_id,
                version_id
            );
        }

        self.written = true;
        Ok(())
    }

    async fn commit(&mut self) -> anyhow::Result<()> {
        self.committed = true;
        Ok(())
    }
}

impl Drop for InnerTxn<'_> {
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

    #[tokio::test]
    async fn test_get_client_empty() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let mut txn = storage.txn(Uuid::new_v4()).await?;
        let maybe_client = txn.get_client().await?;
        assert!(maybe_client.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_client_storage() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;

        let latest_version_id = Uuid::new_v4();
        txn.new_client(latest_version_id).await?;

        let client = txn.get_client().await?.unwrap();
        assert_eq!(client.latest_version_id, latest_version_id);
        assert!(client.snapshot.is_none());

        let latest_version_id = Uuid::new_v4();
        txn.add_version(latest_version_id, Uuid::new_v4(), vec![1, 1])
            .await?;

        let client = txn.get_client().await?.unwrap();
        assert_eq!(client.latest_version_id, latest_version_id);
        assert!(client.snapshot.is_none());

        let snap = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            versions_since: 4,
        };
        txn.set_snapshot(snap.clone(), vec![1, 2, 3]).await?;

        let client = txn.get_client().await?.unwrap();
        assert_eq!(client.latest_version_id, latest_version_id);
        assert_eq!(client.snapshot.unwrap(), snap);

        txn.commit().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_gvbp_empty() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;
        let maybe_version = txn.get_version_by_parent(Uuid::new_v4()).await?;
        assert!(maybe_version.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_add_version_and_get_version() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;

        let version_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let history_segment = b"abc".to_vec();

        txn.new_client(parent_version_id).await?;
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

        txn.commit().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_add_version_exists() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;

        let version_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let history_segment = b"abc".to_vec();

        txn.new_client(parent_version_id).await?;
        txn.add_version(version_id, parent_version_id, history_segment.clone())
            .await?;
        assert!(txn
            .add_version(version_id, parent_version_id, history_segment.clone())
            .await
            .is_err());
        txn.commit().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshots() -> anyhow::Result<()> {
        let storage = InMemoryStorage::new();
        let client_id = Uuid::new_v4();
        let mut txn = storage.txn(client_id).await?;

        txn.new_client(Uuid::new_v4()).await?;
        assert!(txn.get_client().await?.unwrap().snapshot.is_none());

        let snap = Snapshot {
            version_id: Uuid::new_v4(),
            timestamp: Utc::now(),
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
            timestamp: Utc::now(),
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

        txn.commit().await?;
        Ok(())
    }
}
