use actix_web::{error, web, HttpRequest, Result, Scope};
use taskchampion_sync_server_core::{ClientId, Server, ServerError};

use crate::web::WebConfig;

mod add_snapshot;
mod add_version;
mod get_child_version;
mod get_snapshot;

/// The content-type for history segments (opaque blobs of bytes)
pub(crate) const HISTORY_SEGMENT_CONTENT_TYPE: &str =
    "application/vnd.taskchampion.history-segment";

/// The content-type for snapshots (opaque blobs of bytes)
pub(crate) const SNAPSHOT_CONTENT_TYPE: &str = "application/vnd.taskchampion.snapshot";

/// The header name for version ID
pub(crate) const VERSION_ID_HEADER: &str = "X-Version-Id";

/// The header name for client id
pub(crate) const CLIENT_ID_HEADER: &str = "X-Client-Id";

/// The header name for parent version ID
pub(crate) const PARENT_VERSION_ID_HEADER: &str = "X-Parent-Version-Id";

/// The header name for parent version ID
pub(crate) const SNAPSHOT_REQUEST_HEADER: &str = "X-Snapshot-Request";

/// The type containing a reference to the persistent state for the server
pub(crate) struct ServerState {
    pub(crate) server: Server,
    pub(crate) web_config: WebConfig,
}

impl ServerState {
    /// Get the client id
    fn client_id_header(&self, req: &HttpRequest) -> Result<ClientId> {
        fn badrequest() -> error::Error {
            error::ErrorBadRequest("bad x-client-id")
        }
        if let Some(client_id_hdr) = req.headers().get(CLIENT_ID_HEADER) {
            let client_id = client_id_hdr.to_str().map_err(|_| badrequest())?;
            let client_id = ClientId::parse_str(client_id).map_err(|_| badrequest())?;
            if let Some(allow_list) = &self.web_config.client_id_allowlist {
                if !allow_list.contains(&client_id) {
                    return Err(error::ErrorForbidden("unknown x-client-id"));
                }
            }
            Ok(client_id)
        } else {
            Err(badrequest())
        }
    }
}

pub(crate) fn api_scope() -> Scope {
    web::scope("")
        .service(get_child_version::service)
        .service(add_version::service)
        .service(get_snapshot::service)
        .service(add_snapshot::service)
}

/// Convert a `anyhow::Error` to an Actix ISE
fn failure_to_ise(err: anyhow::Error) -> actix_web::Error {
    error::ErrorInternalServerError(err)
}

/// Convert a ServerError to an Actix error
fn server_error_to_actix(err: ServerError) -> actix_web::Error {
    match err {
        ServerError::NoSuchClient => error::ErrorNotFound(err),
        ServerError::Other(err) => error::ErrorInternalServerError(err),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use taskchampion_sync_server_core::InMemoryStorage;
    use uuid::Uuid;

    #[test]
    fn client_id_header_allow_all() {
        let client_id = Uuid::new_v4();
        let state = ServerState {
            server: Server::new(Default::default(), InMemoryStorage::new()),
            web_config: WebConfig {
                client_id_allowlist: None,
                create_clients: true,
                ..WebConfig::default()
            },
        };
        let req = actix_web::test::TestRequest::default()
            .insert_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_http_request();
        assert_eq!(state.client_id_header(&req).unwrap(), client_id);
    }

    #[test]
    fn client_id_header_allow_list() {
        let client_id_ok = Uuid::new_v4();
        let client_id_disallowed = Uuid::new_v4();
        let state = ServerState {
            server: Server::new(Default::default(), InMemoryStorage::new()),
            web_config: WebConfig {
                client_id_allowlist: Some([client_id_ok].into()),
                create_clients: true,
                ..WebConfig::default()
            },
        };
        let req = actix_web::test::TestRequest::default()
            .insert_header((CLIENT_ID_HEADER, client_id_ok.to_string()))
            .to_http_request();
        assert_eq!(state.client_id_header(&req).unwrap(), client_id_ok);
        let req = actix_web::test::TestRequest::default()
            .insert_header((CLIENT_ID_HEADER, client_id_disallowed.to_string()))
            .to_http_request();
        assert_eq!(
            state
                .client_id_header(&req)
                .unwrap_err()
                .as_response_error()
                .status_code(),
            403
        );
    }
}
