#![deny(clippy::all)]

use clap::{arg, builder::ValueParser, ArgMatches, Command};
use std::ffi::OsString;
use taskchampion_sync_server::{args, web};
use taskchampion_sync_server_storage_sqlite::SqliteStorage;

fn command() -> Command {
    args::command().arg(
        arg!(-d --"data-dir" <DIR> "Directory in which to store data")
            .value_parser(ValueParser::os_string())
            .env("DATA_DIR")
            .default_value("/var/lib/taskchampion-sync-server"),
    )
}

fn data_dir_from_matches(matches: &ArgMatches) -> OsString {
    matches.get_one::<OsString>("data-dir").unwrap().clone()
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let matches = command().get_matches();
    let server_config = args::server_config_from_matches(&matches);
    let web_config = args::web_config_from_matches(&matches);
    let data_dir = data_dir_from_matches(&matches);
    let storage = SqliteStorage::new(data_dir)?;

    let server = web::WebServer::new(server_config, web_config, storage);
    server.run().await
}

#[cfg(test)]
mod test {
    use super::*;
    use temp_env::{with_var, with_var_unset};

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
            assert_eq!(data_dir_from_matches(&matches), "/foo/bar");
        });
    }

    #[test]
    fn command_data_dir_env() {
        with_var("DATA_DIR", Some("/foo/bar"), || {
            let matches = command().get_matches_from(["tss", "--listen", "localhost:8080"]);
            assert_eq!(data_dir_from_matches(&matches), "/foo/bar");
        });
    }
}
