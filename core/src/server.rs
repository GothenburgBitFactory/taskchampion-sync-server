use crate::error::ServerError;
use crate::storage::{Client, Snapshot, Storage, StorageTxn};
use chrono::Utc;
use uuid::Uuid;

/// The distinguished value for "no version"
pub const NIL_VERSION_ID: VersionId = Uuid::nil();

/// Number of versions to search back from the latest to find the
/// version for a newly-added snapshot.  Snapshots for versions older
/// than this will be rejected.
const SNAPSHOT_SEARCH_LEN: i32 = 5;

pub type HistorySegment = Vec<u8>;
pub type ClientId = Uuid;
pub type VersionId = Uuid;

/// ServerConfig contains configuration parameters for the server.
pub struct ServerConfig {
    /// Target number of days between snapshots.
    pub snapshot_days: i64,

    /// Target number of versions between snapshots.
    pub snapshot_versions: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            snapshot_days: 14,
            snapshot_versions: 100,
        }
    }
}

/// Response to get_child_version.  See the protocol documentation.
#[derive(Clone, PartialEq, Debug)]
pub enum GetVersionResult {
    NotFound,
    Gone,
    Success {
        version_id: Uuid,
        parent_version_id: Uuid,
        history_segment: HistorySegment,
    },
}

/// Response to add_version
#[derive(Clone, PartialEq, Debug)]
pub enum AddVersionResult {
    /// OK, version added with the given ID
    Ok(VersionId),
    /// Rejected; expected a version with the given parent version
    ExpectedParentVersion(VersionId),
}

/// Urgency of a snapshot for a client; used to create the `X-Snapshot-Request` header.
#[derive(PartialEq, Debug, Clone, Copy, Eq, PartialOrd, Ord)]
pub enum SnapshotUrgency {
    /// Don't need a snapshot right now.
    None,

    /// A snapshot would be good, but can wait for other replicas to provide it.
    Low,

    /// A snapshot is needed right now.
    High,
}

impl SnapshotUrgency {
    /// Calculate the urgency for a snapshot based on its age in days
    fn for_days(config: &ServerConfig, days: i64) -> Self {
        if days >= config.snapshot_days * 3 / 2 {
            SnapshotUrgency::High
        } else if days >= config.snapshot_days {
            SnapshotUrgency::Low
        } else {
            SnapshotUrgency::None
        }
    }

    /// Calculate the urgency for a snapshot based on its age in versions
    fn for_versions_since(config: &ServerConfig, versions_since: u32) -> Self {
        if versions_since >= config.snapshot_versions * 3 / 2 {
            SnapshotUrgency::High
        } else if versions_since >= config.snapshot_versions {
            SnapshotUrgency::Low
        } else {
            SnapshotUrgency::None
        }
    }
}

/// A server implementing the TaskChampion sync protocol.
pub struct Server {
    config: ServerConfig,
    storage: Box<dyn Storage>,
}

impl Server {
    pub fn new<ST: Storage + 'static>(config: ServerConfig, storage: ST) -> Self {
        Self {
            config,
            storage: Box::new(storage),
        }
    }

    /// Implementation of the GetChildVersion protocol transaction.
    pub fn get_child_version(
        &self,
        client_id: ClientId,
        client: Client,
        parent_version_id: VersionId,
    ) -> Result<GetVersionResult, ServerError> {
        let mut txn = self.storage.txn()?;

        // If a version with parentVersionId equal to the requested parentVersionId exists, it is
        // returned.
        if let Some(version) = txn.get_version_by_parent(client_id, parent_version_id)? {
            return Ok(GetVersionResult::Success {
                version_id: version.version_id,
                parent_version_id: version.parent_version_id,
                history_segment: version.history_segment,
            });
        }

        // Return NotFound if an AddVersion with this parent_version_id would succeed, and
        // otherwise return Gone.
        //
        // AddVersion will succeed if either
        //  - the requested parent version is the latest version; or
        //  - there is no latest version, meaning there are no versions stored for this client
        Ok(
            if client.latest_version_id == parent_version_id
                || client.latest_version_id == NIL_VERSION_ID
            {
                GetVersionResult::NotFound
            } else {
                GetVersionResult::Gone
            },
        )
    }

    /// Implementation of the AddVersion protocol transaction
    pub fn add_version(
        &self,
        client_id: ClientId,
        client: Client,
        parent_version_id: VersionId,
        history_segment: HistorySegment,
    ) -> Result<(AddVersionResult, SnapshotUrgency), ServerError> {
        let mut txn = self.storage.txn()?;

        log::debug!(
            "add_version(client_id: {}, parent_version_id: {})",
            client_id,
            parent_version_id,
        );

        // check if this version is acceptable, under the protection of the transaction
        if client.latest_version_id != NIL_VERSION_ID
            && parent_version_id != client.latest_version_id
        {
            log::debug!("add_version request rejected: mismatched latest_version_id");
            return Ok((
                AddVersionResult::ExpectedParentVersion(client.latest_version_id),
                SnapshotUrgency::None,
            ));
        }

        // invent a version ID
        let version_id = Uuid::new_v4();
        log::debug!(
            "add_version request accepted: new version_id: {}",
            version_id
        );

        // update the DB
        txn.add_version(client_id, version_id, parent_version_id, history_segment)?;
        txn.commit()?;

        // calculate the urgency
        let time_urgency = match client.snapshot {
            None => SnapshotUrgency::High,
            Some(Snapshot { timestamp, .. }) => {
                SnapshotUrgency::for_days(&self.config, (Utc::now() - timestamp).num_days())
            }
        };

        let version_urgency = match client.snapshot {
            None => SnapshotUrgency::High,
            Some(Snapshot { versions_since, .. }) => {
                SnapshotUrgency::for_versions_since(&self.config, versions_since)
            }
        };

        Ok((
            AddVersionResult::Ok(version_id),
            std::cmp::max(time_urgency, version_urgency),
        ))
    }

    /// Implementation of the AddSnapshot protocol transaction
    pub fn add_snapshot(
        &self,
        client_id: ClientId,
        client: Client,
        version_id: VersionId,
        data: Vec<u8>,
    ) -> Result<(), ServerError> {
        let mut txn = self.storage.txn()?;

        log::debug!(
            "add_snapshot(client_id: {}, version_id: {})",
            client_id,
            version_id,
        );

        // NOTE: if the snapshot is rejected, this function logs about it and returns
        // Ok(()), as there's no reason to report an errot to the client / user.

        let last_snapshot = client.snapshot.map(|snap| snap.version_id);
        if Some(version_id) == last_snapshot {
            log::debug!(
                "rejecting snapshot for version {}: already exists",
                version_id
            );
            return Ok(());
        }

        // look for this version in the history of this client, starting at the latest version, and
        // only iterating for a limited number of versions.
        let mut search_len = SNAPSHOT_SEARCH_LEN;
        let mut vid = client.latest_version_id;

        loop {
            if vid == version_id && version_id != NIL_VERSION_ID {
                // the new snapshot is for a recent version, so proceed
                break;
            }

            if Some(vid) == last_snapshot {
                // the new snapshot is older than the last snapshot, so ignore it
                log::debug!(
                "rejecting snapshot for version {}: newer snapshot already exists or no such version",
                version_id
            );
                return Ok(());
            }

            search_len -= 1;
            if search_len <= 0 || vid == NIL_VERSION_ID {
                // this should not happen in normal operation, so warn about it
                log::warn!(
                    "rejecting snapshot for version {}: version is too old or no such version",
                    version_id
                );
                return Ok(());
            }

            // get the parent version ID
            if let Some(parent) = txn.get_version(client_id, vid)? {
                vid = parent.parent_version_id;
            } else {
                // this version does not exist; "this should not happen" but if it does,
                // we don't need a snapshot earlier than the missing version.
                log::warn!(
                    "rejecting snapshot for version {}: newer versions have already been deleted",
                    version_id
                );
                return Ok(());
            }
        }

        log::debug!("accepting snapshot for version {}", version_id);
        txn.set_snapshot(
            client_id,
            Snapshot {
                version_id,
                timestamp: Utc::now(),
                versions_since: 0,
            },
            data,
        )?;
        txn.commit()?;
        Ok(())
    }

    /// Implementation of the GetSnapshot protocol transaction
    pub fn get_snapshot(
        &self,
        client_id: ClientId,
        client: Client,
    ) -> Result<Option<(Uuid, Vec<u8>)>, ServerError> {
        let mut txn = self.storage.txn()?;

        Ok(if let Some(snap) = client.snapshot {
            txn.get_snapshot_data(client_id, snap.version_id)?
                .map(|data| (snap.version_id, data))
        } else {
            None
        })
    }

    /// Convenience method to get a transaction for the embedded storage.
    pub fn txn(&self) -> Result<Box<dyn StorageTxn + '_>, ServerError> {
        Ok(self.storage.txn()?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::inmemory::InMemoryStorage;
    use crate::storage::{Snapshot, Storage, StorageTxn};
    use chrono::{Duration, TimeZone, Utc};
    use pretty_assertions::assert_eq;

    fn setup<INIT, RES>(init: INIT) -> anyhow::Result<(Server, RES)>
    where
        INIT: FnOnce(&mut dyn StorageTxn) -> anyhow::Result<RES>,
    {
        let _ = env_logger::builder().is_test(true).try_init();
        let storage = InMemoryStorage::new();
        let res = init(storage.txn()?.as_mut())?;
        Ok((Server::new(ServerConfig::default(), storage), res))
    }

    /// Utility setup function for add_version tests
    fn av_setup(
        num_versions: u32,
        snapshot_version: Option<u32>,
        snapshot_days_ago: Option<i64>,
    ) -> anyhow::Result<(Server, Uuid, Client, Vec<Uuid>)> {
        let (server, (client_id, client, versions)) = setup(|txn| {
            let client_id = Uuid::new_v4();
            let mut versions = vec![];

            let mut version_id = Uuid::nil();
            txn.new_client(client_id, Uuid::nil())?;
            for vnum in 0..num_versions {
                let parent_version_id = version_id;
                version_id = Uuid::new_v4();
                versions.push(version_id);
                txn.add_version(
                    client_id,
                    version_id,
                    parent_version_id,
                    vec![0, 0, vnum as u8],
                )?;
                if Some(vnum) == snapshot_version {
                    txn.set_snapshot(
                        client_id,
                        Snapshot {
                            version_id,
                            versions_since: 0,
                            timestamp: Utc::now() - Duration::days(snapshot_days_ago.unwrap_or(0)),
                        },
                        vec![vnum as u8],
                    )?;
                }
            }

            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client, versions))
        })?;
        Ok((server, client_id, client, versions))
    }

    /// Utility function to check the results of an add_version call
    fn av_success_check(
        server: &Server,
        client_id: Uuid,
        existing_versions: &[Uuid],
        result: (AddVersionResult, SnapshotUrgency),
        expected_history: Vec<u8>,
        expected_urgency: SnapshotUrgency,
    ) -> anyhow::Result<()> {
        if let AddVersionResult::Ok(new_version_id) = result.0 {
            // check that it invented a new version ID
            for v in existing_versions {
                assert_ne!(&new_version_id, v);
            }

            // verify that the storage was updated
            let mut txn = server.txn()?;
            let client = txn.get_client(client_id)?.unwrap();
            assert_eq!(client.latest_version_id, new_version_id);

            let parent_version_id = existing_versions.last().cloned().unwrap_or_else(Uuid::nil);
            let version = txn.get_version(client_id, new_version_id)?.unwrap();
            assert_eq!(version.version_id, new_version_id);
            assert_eq!(version.parent_version_id, parent_version_id);
            assert_eq!(version.history_segment, expected_history);
        } else {
            panic!("did not get Ok from add_version: {:?}", result);
        }

        assert_eq!(result.1, expected_urgency);

        Ok(())
    }

    #[test]
    fn snapshot_urgency_max() {
        use SnapshotUrgency::*;
        assert_eq!(std::cmp::max(None, None), None);
        assert_eq!(std::cmp::max(None, Low), Low);
        assert_eq!(std::cmp::max(None, High), High);
        assert_eq!(std::cmp::max(Low, None), Low);
        assert_eq!(std::cmp::max(Low, Low), Low);
        assert_eq!(std::cmp::max(Low, High), High);
        assert_eq!(std::cmp::max(High, None), High);
        assert_eq!(std::cmp::max(High, Low), High);
        assert_eq!(std::cmp::max(High, High), High);
    }

    #[test]
    fn snapshot_urgency_for_days() {
        use SnapshotUrgency::*;
        let config = ServerConfig::default();
        assert_eq!(SnapshotUrgency::for_days(&config, 0), None);
        assert_eq!(
            SnapshotUrgency::for_days(&config, config.snapshot_days),
            Low
        );
        assert_eq!(
            SnapshotUrgency::for_days(&config, config.snapshot_days * 2),
            High
        );
    }

    #[test]
    fn snapshot_urgency_for_versions_since() {
        use SnapshotUrgency::*;
        let config = ServerConfig::default();
        assert_eq!(SnapshotUrgency::for_versions_since(&config, 0), None);
        assert_eq!(
            SnapshotUrgency::for_versions_since(&config, config.snapshot_versions),
            Low
        );
        assert_eq!(
            SnapshotUrgency::for_versions_since(&config, config.snapshot_versions * 2),
            High
        );
    }

    #[test]
    fn get_child_version_not_found_initial_nil() -> anyhow::Result<()> {
        let (server, (client_id, client)) = setup(|txn| {
            let client_id = Uuid::new_v4();
            txn.new_client(client_id, NIL_VERSION_ID)?;

            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client))
        })?;
        // when no latest version exists, the first version is NotFound
        assert_eq!(
            server.get_child_version(client_id, client, NIL_VERSION_ID)?,
            GetVersionResult::NotFound
        );
        Ok(())
    }

    #[test]
    fn get_child_version_not_found_initial_continuing() -> anyhow::Result<()> {
        let (server, (client_id, client)) = setup(|txn| {
            let client_id = Uuid::new_v4();
            txn.new_client(client_id, NIL_VERSION_ID)?;

            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client))
        })?;

        // when no latest version exists, _any_ child version is NOT_FOUND. This allows syncs to
        // start to a new server even if the client already has been uploading to another service.
        assert_eq!(
            server.get_child_version(client_id, client, Uuid::new_v4(),)?,
            GetVersionResult::NotFound
        );
        Ok(())
    }

    #[test]
    fn get_child_version_not_found_up_to_date() -> anyhow::Result<()> {
        let (server, (client_id, client, parent_version_id)) = setup(|txn| {
            // add a parent version, but not the requested child version
            let client_id = Uuid::new_v4();
            let parent_version_id = Uuid::new_v4();
            txn.new_client(client_id, parent_version_id)?;
            txn.add_version(client_id, parent_version_id, NIL_VERSION_ID, vec![])?;

            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client, parent_version_id))
        })?;

        assert_eq!(
            server.get_child_version(client_id, client, parent_version_id)?,
            GetVersionResult::NotFound
        );
        Ok(())
    }

    #[test]
    fn get_child_version_gone_not_latest() -> anyhow::Result<()> {
        let (server, (client_id, client)) = setup(|txn| {
            let client_id = Uuid::new_v4();

            // Add a parent version, but not the requested parent version
            let parent_version_id = Uuid::new_v4();
            txn.new_client(client_id, parent_version_id)?;
            txn.add_version(client_id, parent_version_id, NIL_VERSION_ID, vec![])?;

            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client))
        })?;

        assert_eq!(
            server.get_child_version(client_id, client, Uuid::new_v4(),)?,
            GetVersionResult::Gone
        );
        Ok(())
    }

    #[test]
    fn get_child_version_found() -> anyhow::Result<()> {
        let (server, (client_id, client, version_id, parent_version_id, history_segment)) =
            setup(|txn| {
                let client_id = Uuid::new_v4();
                let version_id = Uuid::new_v4();
                let parent_version_id = Uuid::new_v4();
                let history_segment = b"abcd".to_vec();

                txn.new_client(client_id, version_id)?;
                txn.add_version(
                    client_id,
                    version_id,
                    parent_version_id,
                    history_segment.clone(),
                )?;

                let client = txn.get_client(client_id)?.unwrap();
                Ok((
                    client_id,
                    client,
                    version_id,
                    parent_version_id,
                    history_segment,
                ))
            })?;
        assert_eq!(
            server.get_child_version(client_id, client, parent_version_id)?,
            GetVersionResult::Success {
                version_id,
                parent_version_id,
                history_segment,
            }
        );
        Ok(())
    }

    #[test]
    fn add_version_conflict() -> anyhow::Result<()> {
        let (server, client_id, client, versions) = av_setup(3, None, None)?;

        // try to add a child of a version other than the latest
        assert_eq!(
            server
                .add_version(client_id, client, versions[1], vec![3, 6, 9])?
                .0,
            AddVersionResult::ExpectedParentVersion(versions[2])
        );

        // verify that the storage wasn't updated
        let mut txn = server.txn()?;
        assert_eq!(
            txn.get_client(client_id)?.unwrap().latest_version_id,
            versions[2]
        );
        assert_eq!(txn.get_version_by_parent(client_id, versions[2])?, None);

        Ok(())
    }

    #[test]
    fn add_version_with_existing_history() -> anyhow::Result<()> {
        let (server, client_id, client, versions) = av_setup(1, None, None)?;

        let result = server.add_version(client_id, client, versions[0], vec![3, 6, 9])?;

        av_success_check(
            &server,
            client_id,
            &versions,
            result,
            vec![3, 6, 9],
            // urgency=high because there are no snapshots yet
            SnapshotUrgency::High,
        )?;

        Ok(())
    }

    #[test]
    fn add_version_with_no_history() -> anyhow::Result<()> {
        let (server, client_id, client, versions) = av_setup(0, None, None)?;

        let parent_version_id = Uuid::nil();
        let result = server.add_version(client_id, client, parent_version_id, vec![3, 6, 9])?;

        av_success_check(
            &server,
            client_id,
            &versions,
            result,
            vec![3, 6, 9],
            // urgency=high because there are no snapshots yet
            SnapshotUrgency::High,
        )?;

        Ok(())
    }

    #[test]
    fn add_version_success_recent_snapshot() -> anyhow::Result<()> {
        let (server, client_id, client, versions) = av_setup(1, Some(0), None)?;

        let result = server.add_version(client_id, client, versions[0], vec![1, 2, 3])?;

        av_success_check(
            &server,
            client_id,
            &versions,
            result,
            vec![1, 2, 3],
            // no snapshot request since the previous version has a snapshot
            SnapshotUrgency::None,
        )?;

        Ok(())
    }

    #[test]
    fn add_version_success_aged_snapshot() -> anyhow::Result<()> {
        // one snapshot, but it was 50 days ago
        let (server, client_id, client, versions) = av_setup(1, Some(0), Some(50))?;

        let result = server.add_version(client_id, client, versions[0], vec![1, 2, 3])?;

        av_success_check(
            &server,
            client_id,
            &versions,
            result,
            vec![1, 2, 3],
            // urgency=high due to days since the snapshot
            SnapshotUrgency::High,
        )?;

        Ok(())
    }

    #[test]
    fn add_version_success_snapshot_many_versions_ago() -> anyhow::Result<()> {
        // one snapshot, but it was 50 versions ago
        let (mut server, client_id, client, versions) = av_setup(50, Some(0), None)?;
        server.config.snapshot_versions = 30;

        let result = server.add_version(client_id, client, versions[49], vec![1, 2, 3])?;

        av_success_check(
            &server,
            client_id,
            &versions,
            result,
            vec![1, 2, 3],
            // urgency=high due to number of versions since the snapshot
            SnapshotUrgency::High,
        )?;

        Ok(())
    }

    #[test]
    fn add_snapshot_success_latest() -> anyhow::Result<()> {
        let (server, (client_id, client, version_id)) = setup(|txn| {
            let client_id = Uuid::new_v4();
            let version_id = Uuid::new_v4();

            // set up a task DB with one version in it
            txn.new_client(client_id, version_id)?;
            txn.add_version(client_id, version_id, NIL_VERSION_ID, vec![])?;

            // add a snapshot for that version
            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client, version_id))
        })?;
        server.add_snapshot(client_id, client, version_id, vec![1, 2, 3])?;

        // verify the snapshot
        let mut txn = server.txn()?;
        let client = txn.get_client(client_id)?.unwrap();
        let snapshot = client.snapshot.unwrap();
        assert_eq!(snapshot.version_id, version_id);
        assert_eq!(snapshot.versions_since, 0);
        assert_eq!(
            txn.get_snapshot_data(client_id, version_id).unwrap(),
            Some(vec![1, 2, 3])
        );

        Ok(())
    }

    #[test]
    fn add_snapshot_success_older() -> anyhow::Result<()> {
        let (server, (client_id, client, version_id_1)) = setup(|txn| {
            let client_id = Uuid::new_v4();
            let version_id_1 = Uuid::new_v4();
            let version_id_2 = Uuid::new_v4();

            // set up a task DB with two versions in it
            txn.new_client(client_id, version_id_2)?;
            txn.add_version(client_id, version_id_1, NIL_VERSION_ID, vec![])?;
            txn.add_version(client_id, version_id_2, version_id_1, vec![])?;

            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client, version_id_1))
        })?;
        // add a snapshot for version 1
        server.add_snapshot(client_id, client, version_id_1, vec![1, 2, 3])?;

        // verify the snapshot
        let mut txn = server.txn()?;
        let client = txn.get_client(client_id)?.unwrap();
        let snapshot = client.snapshot.unwrap();
        assert_eq!(snapshot.version_id, version_id_1);
        assert_eq!(snapshot.versions_since, 0);
        assert_eq!(
            txn.get_snapshot_data(client_id, version_id_1).unwrap(),
            Some(vec![1, 2, 3])
        );

        Ok(())
    }

    #[test]
    fn add_snapshot_fails_no_such() -> anyhow::Result<()> {
        let (server, (client_id, client)) = setup(|txn| {
            let client_id = Uuid::new_v4();
            let version_id_1 = Uuid::new_v4();
            let version_id_2 = Uuid::new_v4();

            // set up a task DB with two versions in it
            txn.new_client(client_id, version_id_2)?;
            txn.add_version(client_id, version_id_1, NIL_VERSION_ID, vec![])?;
            txn.add_version(client_id, version_id_2, version_id_1, vec![])?;

            // add a snapshot for unknown version
            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client))
        })?;

        let version_id_unk = Uuid::new_v4();
        server.add_snapshot(client_id, client, version_id_unk, vec![1, 2, 3])?;

        // verify the snapshot does not exist
        let mut txn = server.txn()?;
        let client = txn.get_client(client_id)?.unwrap();
        assert!(client.snapshot.is_none());

        Ok(())
    }

    #[test]
    fn add_snapshot_fails_too_old() -> anyhow::Result<()> {
        let (server, (client_id, client, version_ids)) = setup(|txn| {
            let client_id = Uuid::new_v4();
            let mut version_id = Uuid::new_v4();
            let mut parent_version_id = Uuid::nil();
            let mut version_ids = vec![];

            // set up a task DB with 10 versions in it (oldest to newest)
            txn.new_client(client_id, Uuid::nil())?;
            for _ in 0..10 {
                txn.add_version(client_id, version_id, parent_version_id, vec![])?;
                version_ids.push(version_id);
                parent_version_id = version_id;
                version_id = Uuid::new_v4();
            }

            // add a snapshot for the earliest of those
            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client, version_ids))
        })?;
        server.add_snapshot(client_id, client, version_ids[0], vec![1, 2, 3])?;

        // verify the snapshot does not exist
        let mut txn = server.txn()?;
        let client = txn.get_client(client_id)?.unwrap();
        assert!(client.snapshot.is_none());

        Ok(())
    }

    #[test]
    fn add_snapshot_fails_newer_exists() -> anyhow::Result<()> {
        let (server, (client_id, client, version_ids)) = setup(|txn| {
            let client_id = Uuid::new_v4();
            let mut version_id = Uuid::new_v4();
            let mut parent_version_id = Uuid::nil();
            let mut version_ids = vec![];

            // set up a task DB with 5 versions in it (oldest to newest) and a snapshot of the
            // middle one
            txn.new_client(client_id, Uuid::nil())?;
            for _ in 0..5 {
                txn.add_version(client_id, version_id, parent_version_id, vec![])?;
                version_ids.push(version_id);
                parent_version_id = version_id;
                version_id = Uuid::new_v4();
            }
            txn.set_snapshot(
                client_id,
                Snapshot {
                    version_id: version_ids[2],
                    versions_since: 2,
                    timestamp: Utc.with_ymd_and_hms(2001, 9, 9, 1, 46, 40).unwrap(),
                },
                vec![1, 2, 3],
            )?;

            // add a snapshot for the earliest of those
            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client, version_ids))
        })?;

        server.add_snapshot(client_id, client, version_ids[0], vec![9, 9, 9])?;

        // verify the snapshot was not replaced
        let mut txn = server.txn()?;
        let client = txn.get_client(client_id)?.unwrap();
        let snapshot = client.snapshot.unwrap();
        assert_eq!(snapshot.version_id, version_ids[2]);
        assert_eq!(snapshot.versions_since, 2);
        assert_eq!(
            txn.get_snapshot_data(client_id, version_ids[2]).unwrap(),
            Some(vec![1, 2, 3])
        );

        Ok(())
    }

    #[test]
    fn add_snapshot_fails_nil_version() -> anyhow::Result<()> {
        let (server, (client_id, client)) = setup(|txn| {
            let client_id = Uuid::new_v4();

            // just set up the client
            txn.new_client(client_id, NIL_VERSION_ID)?;

            // add a snapshot for the nil version
            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client))
        })?;

        server.add_snapshot(client_id, client, NIL_VERSION_ID, vec![9, 9, 9])?;

        // verify the snapshot does not exist
        let mut txn = server.txn()?;
        let client = txn.get_client(client_id)?.unwrap();
        assert!(client.snapshot.is_none());

        Ok(())
    }

    #[test]
    fn get_snapshot_found() -> anyhow::Result<()> {
        let (server, (client_id, client, data, snapshot_version_id)) = setup(|txn| {
            let client_id = Uuid::new_v4();
            let data = vec![1, 2, 3];
            let snapshot_version_id = Uuid::new_v4();

            txn.new_client(client_id, snapshot_version_id)?;
            txn.set_snapshot(
                client_id,
                Snapshot {
                    version_id: snapshot_version_id,
                    versions_since: 3,
                    timestamp: Utc.with_ymd_and_hms(2001, 9, 9, 1, 46, 40).unwrap(),
                },
                data.clone(),
            )?;
            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client, data, snapshot_version_id))
        })?;
        assert_eq!(
            server.get_snapshot(client_id, client)?,
            Some((snapshot_version_id, data))
        );

        Ok(())
    }

    #[test]
    fn get_snapshot_not_found() -> anyhow::Result<()> {
        let (server, (client_id, client)) = setup(|txn| {
            let client_id = Uuid::new_v4();
            txn.new_client(client_id, NIL_VERSION_ID)?;
            let client = txn.get_client(client_id)?.unwrap();
            Ok((client_id, client))
        })?;

        assert_eq!(server.get_snapshot(client_id, client)?, None);

        Ok(())
    }
}
