#![deny(clippy::all)]

use clap::{arg, builder::ValueParser, ArgMatches, Command};
use std::ffi::OsString;
use taskchampion_sync_server::{args, web};
use taskchampion_sync_server_storage_postgres::PostgresStorage;

fn command() -> Command {
    args::command().arg(
        arg!(-c --"connection" <DIR> "LibPQ-style connection URI")
            .value_parser(ValueParser::os_string())
            .help("See https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING-URIS")
            .required(true)
            .env("CONNECTION")
    )
}

fn connection_from_matches(matches: &ArgMatches) -> String {
    matches
        .get_one::<OsString>("connection")
        .unwrap()
        .to_str()
        .expect("--connection must be valid UTF-8")
        .to_string()
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let matches = command().get_matches();
    let server_config = args::server_config_from_matches(&matches);
    let web_config = args::web_config_from_matches(&matches);
    let connection = connection_from_matches(&matches);
    let storage = PostgresStorage::new(connection).await?;

    let server = web::WebServer::new(server_config, web_config, storage);
    server.run().await
}

#[cfg(test)]
mod test {
    use super::*;
    use temp_env::{with_var, with_var_unset};

    #[test]
    fn command_connection() {
        with_var_unset("CONNECTION", || {
            let matches = command().get_matches_from([
                "tss",
                "--connection",
                "postgresql:/foo/bar",
                "--listen",
                "localhost:8080",
            ]);
            assert_eq!(connection_from_matches(&matches), "postgresql:/foo/bar");
        });
    }

    #[test]
    fn command_connection_env() {
        with_var("CONNECTION", Some("postgresql:/foo/bar"), || {
            let matches = command().get_matches_from(["tss", "--listen", "localhost:8080"]);
            assert_eq!(connection_from_matches(&matches), "postgresql:/foo/bar");
        });
    }
}
