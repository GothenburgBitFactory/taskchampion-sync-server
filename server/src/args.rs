use crate::web::WebConfig;
use clap::{arg, builder::ValueParser, value_parser, ArgAction, ArgMatches, Command};
use taskchampion_sync_server_core::ServerConfig;
use uuid::Uuid;

pub fn command() -> Command {
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
            arg!(-C --"allow-client-id" <CLIENT_ID> "Client IDs to allow (can be repeated; if not specified, all clients are allowed)")
                .value_delimiter(',')
                .value_parser(value_parser!(Uuid))
                .env("CLIENT_ID")
                .action(ArgAction::Append)
                .required(false),
        )
        .arg(
            arg!("create-clients": --"no-create-clients" "If a client does not exist in the database, do not create it")
                .env("CREATE_CLIENTS")
                .default_value("true")
                .action(ArgAction::SetFalse)
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

/// Create a ServerConfig from these args.
pub fn server_config_from_matches(matches: &ArgMatches) -> ServerConfig {
    ServerConfig {
        snapshot_versions: *matches.get_one("snapshot-versions").unwrap(),
        snapshot_days: *matches.get_one("snapshot-days").unwrap(),
    }
}

/// Create a WebConfig from these args.
pub fn web_config_from_matches(matches: &ArgMatches) -> WebConfig {
    WebConfig {
        client_id_allowlist: matches
            .get_many("allow-client-id")
            .map(|ids| ids.copied().collect()),
        create_clients: matches.get_one("create-clients").copied().unwrap_or(true),
        listen_addresses: matches
            .get_many::<String>("listen")
            .unwrap()
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::bool_assert_comparison)]

    use super::*;
    use crate::web::WebServer;
    use actix_web::{self, App};
    use clap::ArgMatches;
    use taskchampion_sync_server_core::InMemoryStorage;
    use temp_env::{with_var, with_var_unset, with_vars, with_vars_unset};

    /// Get the list of allowed client IDs, sorted.
    fn allowed(matches: ArgMatches) -> Option<Vec<Uuid>> {
        web_config_from_matches(&matches)
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
                web_config_from_matches(&matches).listen_addresses,
                vec!["localhost:8080".to_string(), "otherhost:9090".to_string()]
            );
        });
    }

    #[test]
    fn command_listen_two_env() {
        with_var("LISTEN", Some("localhost:8080,otherhost:9090"), || {
            let matches = command().get_matches_from(["tss"]);
            assert_eq!(
                web_config_from_matches(&matches).listen_addresses,
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
            let server_config = server_config_from_matches(&matches);
            assert_eq!(server_config.snapshot_days, 13i64);
            assert_eq!(server_config.snapshot_versions, 20u32);
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
                let server_config = server_config_from_matches(&matches);
                assert_eq!(server_config.snapshot_days, 13i64);
                assert_eq!(server_config.snapshot_versions, 20u32);
            },
        );
    }

    #[test]
    fn command_create_clients_default() {
        with_var_unset("CREATE_CLIENTS", || {
            let matches = command().get_matches_from(["tss", "--listen", "localhost:8080"]);
            let server_config = web_config_from_matches(&matches);
            assert_eq!(server_config.create_clients, true);
        });
    }

    #[test]
    fn command_create_clients_cmdline() {
        with_var_unset("CREATE_CLIENTS", || {
            let matches = command().get_matches_from([
                "tss",
                "--listen",
                "localhost:8080",
                "--no-create-clients",
            ]);
            let server_config = web_config_from_matches(&matches);
            assert_eq!(server_config.create_clients, false);
        });
    }

    #[test]
    fn command_create_clients_env_true() {
        with_vars([("CREATE_CLIENTS", Some("true"))], || {
            let matches = command().get_matches_from(["tss", "--listen", "localhost:8080"]);
            let server_config = web_config_from_matches(&matches);
            assert_eq!(server_config.create_clients, true);
        });
    }

    #[test]
    fn command_create_clients_env_false() {
        with_vars([("CREATE_CLIENTS", Some("false"))], || {
            let matches = command().get_matches_from(["tss", "--listen", "localhost:8080"]);
            let server_config = web_config_from_matches(&matches);
            assert_eq!(server_config.create_clients, false);
        });
    }

    #[actix_rt::test]
    async fn test_index_get() {
        let server = WebServer::new(
            ServerConfig::default(),
            WebConfig::default(),
            InMemoryStorage::new(),
        );
        let app = App::new().configure(|sc| server.config(sc));
        let app = actix_web::test::init_service(app).await;

        let req = actix_web::test::TestRequest::get().uri("/").to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
