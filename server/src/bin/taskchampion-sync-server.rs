#![deny(clippy::all)]

use actix_web::{
    dev::ServiceResponse,
    http::StatusCode,
    middleware::{ErrorHandlerResponse, ErrorHandlers, Logger},
    App, HttpServer,
};
use clap::{arg, builder::ValueParser, value_parser, ArgAction, Command};
use std::{collections::HashSet, ffi::OsString};
use taskchampion_sync_server::WebServer;
use taskchampion_sync_server_core::ServerConfig;
use taskchampion_sync_server_storage_sqlite::SqliteStorage;
use uuid::Uuid;

fn command() -> Command {
    let defaults = ServerConfig::default();
    let default_snapshot_versions = defaults.snapshot_versions.to_string();
    let default_snapshot_days = defaults.snapshot_days.to_string();
    Command::new("taskchampion-sync-server")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Server for TaskChampion")
        .arg(
            arg!(-l --listen <ADDRESS>)
                .help("Address and Port on which to listen on. Can be an IP Address or a DNS name followed by a colon and a port e.g. localhost:8080")
                .value_delimiter(',')
                .value_parser(ValueParser::string())
                .env("LISTEN")
                .action(ArgAction::Append)
                .required(true),
        )
        .arg(
            arg!(-d --"data-dir" <DIR> "Directory in which to store data")
                .value_parser(ValueParser::os_string())
                .env("DATA_DIR")
                .default_value("/var/lib/taskchampion-sync-server"),
        )
        .arg(
            arg!(-C --"allow-client-id" <CLIENT_ID> "Client IDs to allow (can be repeated; if not specified, all clients are allowed)")
                .value_delimiter(',')
                .value_parser(value_parser!(Uuid))
                .env("CLIENT_ID")
                .action(ArgAction::Append)
                .required(false),
        )
        .arg(
            arg!(--"snapshot-versions" <NUM> "Target number of versions between snapshots")
                .value_parser(value_parser!(u32))
                .env("SNAPSHOT_VERSIONS")
                .default_value(default_snapshot_versions),
        )
        .arg(
            arg!(--"snapshot-days" <NUM> "Target number of days between snapshots")
                .value_parser(value_parser!(i64))
                .env("SNAPSHOT_DAYS")
                .default_value(default_snapshot_days),
        )
}

fn print_error<B>(res: ServiceResponse<B>) -> actix_web::Result<ErrorHandlerResponse<B>> {
    if let Some(err) = res.response().error() {
        log::error!("Internal Server Error caused by:\n{:?}", err);
    }
    Ok(ErrorHandlerResponse::Response(res.map_into_left_body()))
}

struct ServerArgs {
    data_dir: OsString,
    snapshot_versions: u32,
    snapshot_days: i64,
    client_id_allowlist: Option<HashSet<Uuid>>,
    listen_addresses: Vec<String>,
}

impl ServerArgs {
    fn new(matches: clap::ArgMatches) -> Self {
        Self {
            data_dir: matches.get_one::<OsString>("data-dir").unwrap().clone(),
            snapshot_versions: *matches.get_one("snapshot-versions").unwrap(),
            snapshot_days: *matches.get_one("snapshot-days").unwrap(),
            client_id_allowlist: matches
                .get_many("allow-client-id")
                .map(|ids| ids.copied().collect()),
            listen_addresses: matches
                .get_many::<String>("listen")
                .unwrap()
                .cloned()
                .collect(),
        }
    }
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let matches = command().get_matches();

    let server_args = ServerArgs::new(matches);
    let config = ServerConfig {
        snapshot_days: server_args.snapshot_days,
        snapshot_versions: server_args.snapshot_versions,
    };
    let server = WebServer::new(
        config,
        server_args.client_id_allowlist,
        SqliteStorage::new(server_args.data_dir)?,
    );

    let mut http_server = HttpServer::new(move || {
        App::new()
            .wrap(ErrorHandlers::new().handler(StatusCode::INTERNAL_SERVER_ERROR, print_error))
            .wrap(Logger::default())
            .configure(|cfg| server.config(cfg))
    });
    for listen_address in server_args.listen_addresses {
        log::info!("Serving on {}", listen_address);
        http_server = http_server.bind(listen_address)?
    }
    http_server.run().await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use actix_web::{self, App};
    use clap::ArgMatches;
    use taskchampion_sync_server_core::InMemoryStorage;
    use temp_env::{with_var, with_var_unset, with_vars, with_vars_unset};

    /// Get the list of allowed client IDs, sorted.
    fn allowed(matches: ArgMatches) -> Option<Vec<Uuid>> {
        ServerArgs::new(matches)
            .client_id_allowlist
            .map(|ids| ids.into_iter().collect::<Vec<_>>())
            .map(|mut ids| {
                ids.sort();
                ids
            })
    }

    #[test]
    fn command_listen_two() {
        with_var_unset("LISTEN", || {
            let matches = command().get_matches_from([
                "tss",
                "--listen",
                "localhost:8080",
                "--listen",
                "otherhost:9090",
            ]);
            assert_eq!(
                ServerArgs::new(matches).listen_addresses,
                vec!["localhost:8080".to_string(), "otherhost:9090".to_string()]
            );
        });
    }

    #[test]
    fn command_listen_two_env() {
        with_var("LISTEN", Some("localhost:8080,otherhost:9090"), || {
            let matches = command().get_matches_from(["tss"]);
            assert_eq!(
                ServerArgs::new(matches).listen_addresses,
                vec!["localhost:8080".to_string(), "otherhost:9090".to_string()]
            );
        });
    }

    #[test]
    fn command_allowed_client_ids_none() {
        with_var_unset("CLIENT_ID", || {
            let matches = command().get_matches_from(["tss", "--listen", "localhost:8080"]);
            assert_eq!(allowed(matches), None);
        });
    }

    #[test]
    fn command_allowed_client_ids_one() {
        with_var_unset("CLIENT_ID", || {
            let matches = command().get_matches_from([
                "tss",
                "--listen",
                "localhost:8080",
                "-C",
                "711d5cf3-0cf0-4eb8-9eca-6f7f220638c0",
            ]);
            assert_eq!(
                allowed(matches),
                Some(vec![Uuid::parse_str(
                    "711d5cf3-0cf0-4eb8-9eca-6f7f220638c0"
                )
                .unwrap()])
            );
        });
    }

    #[test]
    fn command_allowed_client_ids_one_env() {
        with_var(
            "CLIENT_ID",
            Some("711d5cf3-0cf0-4eb8-9eca-6f7f220638c0"),
            || {
                let matches = command().get_matches_from(["tss", "--listen", "localhost:8080"]);
                assert_eq!(
                    allowed(matches),
                    Some(vec![Uuid::parse_str(
                        "711d5cf3-0cf0-4eb8-9eca-6f7f220638c0"
                    )
                    .unwrap()])
                );
            },
        );
    }

    #[test]
    fn command_allowed_client_ids_two() {
        with_var_unset("CLIENT_ID", || {
            let matches = command().get_matches_from([
                "tss",
                "--listen",
                "localhost:8080",
                "-C",
                "711d5cf3-0cf0-4eb8-9eca-6f7f220638c0",
                "-C",
                "bbaf4b61-344a-4a39-a19e-8caa0669b353",
            ]);
            assert_eq!(
                allowed(matches),
                Some(vec![
                    Uuid::parse_str("711d5cf3-0cf0-4eb8-9eca-6f7f220638c0").unwrap(),
                    Uuid::parse_str("bbaf4b61-344a-4a39-a19e-8caa0669b353").unwrap()
                ])
            );
        });
    }

    #[test]
    fn command_allowed_client_ids_two_env() {
        with_var(
            "CLIENT_ID",
            Some("711d5cf3-0cf0-4eb8-9eca-6f7f220638c0,bbaf4b61-344a-4a39-a19e-8caa0669b353"),
            || {
                let matches = command().get_matches_from(["tss", "--listen", "localhost:8080"]);
                assert_eq!(
                    allowed(matches),
                    Some(vec![
                        Uuid::parse_str("711d5cf3-0cf0-4eb8-9eca-6f7f220638c0").unwrap(),
                        Uuid::parse_str("bbaf4b61-344a-4a39-a19e-8caa0669b353").unwrap()
                    ])
                );
            },
        );
    }

    #[test]
    fn command_data_dir() {
        with_var_unset("DATA_DIR", || {
            let matches = command().get_matches_from([
                "tss",
                "--data-dir",
                "/foo/bar",
                "--listen",
                "localhost:8080",
            ]);
            assert_eq!(ServerArgs::new(matches).data_dir, "/foo/bar");
        });
    }

    #[test]
    fn command_data_dir_env() {
        with_var("DATA_DIR", Some("/foo/bar"), || {
            let matches = command().get_matches_from(["tss", "--listen", "localhost:8080"]);
            assert_eq!(ServerArgs::new(matches).data_dir, "/foo/bar");
        });
    }

    #[test]
    fn command_snapshot() {
        with_vars_unset(["SNAPSHOT_DAYS", "SNAPSHOT_VERSIONS"], || {
            let matches = command().get_matches_from([
                "tss",
                "--listen",
                "localhost:8080",
                "--snapshot-days",
                "13",
                "--snapshot-versions",
                "20",
            ]);
            let server_args = ServerArgs::new(matches);
            assert_eq!(server_args.snapshot_days, 13i64);
            assert_eq!(server_args.snapshot_versions, 20u32);
        });
    }

    #[test]
    fn command_snapshot_env() {
        with_vars(
            [
                ("SNAPSHOT_DAYS", Some("13")),
                ("SNAPSHOT_VERSIONS", Some("20")),
            ],
            || {
                let matches = command().get_matches_from(["tss", "--listen", "localhost:8080"]);
                let server_args = ServerArgs::new(matches);
                assert_eq!(server_args.snapshot_days, 13i64);
                assert_eq!(server_args.snapshot_versions, 20u32);
            },
        );
    }

    #[actix_rt::test]
    async fn test_index_get() {
        let server = WebServer::new(Default::default(), None, InMemoryStorage::new());
        let app = App::new().configure(|sc| server.config(sc));
        let app = actix_web::test::init_service(app).await;

        let req = actix_web::test::TestRequest::get().uri("/").to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
