use std::{future::Future, sync::LazyLock};
use tokio::{sync::Mutex, task};
use tokio_postgres::NoTls;

// An async mutex used to ensure exclusive access to the database.
static DB_LOCK: LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

/// Call the given function with a DB client, pointing to an initialized DB.
///
/// This serializes use of the database so that two tests are not simultaneously
/// modifying it.
///
/// The function's future need not be `Send`.
pub(crate) async fn with_db<F, FUT>(f: F) -> anyhow::Result<()>
where
    F: FnOnce(String, tokio_postgres::Client) -> FUT,
    FUT: Future<Output = anyhow::Result<()>> + 'static,
{
    let _ = env_logger::builder().is_test(true).try_init();

    let Ok(connection_string) = std::env::var("TEST_DB_URL") else {
        // If this is run in a GitHub action, then we really don't want to skip the tests.
        if std::env::var("GITHUB_ACTIONS").is_ok() {
            panic!("TEST_DB_URL must be set in GitHub actions");
        }
        // Skip the test.
        return Ok(());
    };

    // Serialize use of the DB.
    let _db_guard = DB_LOCK.lock().await;

    let local_set = task::LocalSet::new();
    local_set
        .run_until(async move {
            let (client, connection) = tokio_postgres::connect(&connection_string, NoTls).await?;
            let conn_join_handle = tokio::spawn(async move {
                if let Err(e) = connection.await {
                    log::warn!("connection error: {e}");
                }
            });

            // Set up the DB.
            client
                .execute("drop schema if exists public cascade", &[])
                .await?;
            client.execute("create schema public", &[]).await?;
            client.simple_query(include_str!("../schema.sql")).await?;

            // Run the test in its own task, so that we can handle all failure cases. This task must be
            // local because the future typically uses `StorageTxn` which is not `Send`.
            let test_join_handle = tokio::task::spawn_local(f(connection_string.clone(), client));

            // Wait for the test task to complete.
            let test_res = test_join_handle.await?;

            conn_join_handle.await?;

            // Clean up the DB.

            let (client, connection) = tokio_postgres::connect(&connection_string, NoTls).await?;
            let conn_join_handle = tokio::spawn(async move {
                if let Err(e) = connection.await {
                    log::warn!("connection error: {e}");
                }
            });
            client
                .execute("drop schema if exists public cascade", &[])
                .await?;
            drop(client);
            conn_join_handle.await?;

            test_res
        })
        .await
}
