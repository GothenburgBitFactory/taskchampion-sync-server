use std::thread;
use taskchampion_sync_server_core::{Storage, NIL_VERSION_ID};
use taskchampion_sync_server_storage_sqlite::SqliteStorage;
use tempfile::TempDir;
use uuid::Uuid;

/// Test that calls to `add_version` from different threads maintain sequential consistency.
#[test]
fn add_version_concurrency() -> anyhow::Result<()> {
    let tmp_dir = TempDir::new()?;
    let client_id = Uuid::new_v4();

    {
        let con = SqliteStorage::new(tmp_dir.path())?;
        let mut txn = con.txn()?;
        txn.new_client(client_id, NIL_VERSION_ID)?;
        txn.commit()?;
    }

    const N: i32 = 100;
    const T: i32 = 4;

    // Add N versions to the DB.
    let add_versions = || {
        let con = SqliteStorage::new(tmp_dir.path())?;

        for _ in 0..N {
            let mut txn = con.txn()?;
            let client = txn.get_client(client_id)?.unwrap();
            let version_id = Uuid::new_v4();
            let parent_version_id = client.latest_version_id;
            std::thread::yield_now(); // Make failure more likely.
            txn.add_version(client_id, version_id, parent_version_id, b"data".to_vec())?;
            txn.commit()?;
        }

        Ok::<_, anyhow::Error>(())
    };

    thread::scope(|s| {
        // Spawn T threads.
        for _ in 0..T {
            s.spawn(add_versions);
        }
    });

    // There should now be precisely N*T versions. This number will be smaller if there were
    // concurrent transactions, which would have allowed two `add_version` calls with the
    // same `parent_version_id`.
    {
        let con = SqliteStorage::new(tmp_dir.path())?;
        let mut txn = con.txn()?;
        let client = txn.get_client(client_id)?.unwrap();

        let mut n = 0;
        let mut version_id = client.latest_version_id;
        while version_id != NIL_VERSION_ID {
            let version = txn
                .get_version(client_id, version_id)?
                .expect("version should exist");
            n += 1;
            version_id = version.parent_version_id;
        }

        assert_eq!(n, N * T);
    }

    Ok(())
}
