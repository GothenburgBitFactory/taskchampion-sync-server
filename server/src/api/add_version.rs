use crate::api::{
    client_id_header, failure_to_ise, server_error_to_actix, ServerState,
    HISTORY_SEGMENT_CONTENT_TYPE, PARENT_VERSION_ID_HEADER, SNAPSHOT_REQUEST_HEADER,
    VERSION_ID_HEADER,
};
use actix_web::{error, post, web, HttpMessage, HttpRequest, HttpResponse, Result};
use futures::StreamExt;
use std::sync::Arc;
use taskchampion_sync_server_core::{
    AddVersionResult, ServerError, SnapshotUrgency, VersionId, NIL_VERSION_ID,
};

/// Max history segment size: 100MB
const MAX_SIZE: usize = 100 * 1024 * 1024;

/// Add a new version, after checking prerequisites.  The history segment should be transmitted in
/// the request entity body and must have content-type
/// `application/vnd.taskchampion.history-segment`.  The content can be encoded in any of the
/// formats supported by actix-web.
///
/// On success, the response is a 200 OK with the new version ID in the `X-Version-Id` header.  If
/// the version cannot be added due to a conflict, the response is a 409 CONFLICT with the expected
/// parent version ID in the `X-Parent-Version-Id` header.
///
/// If included, a snapshot request appears in the `X-Snapshot-Request` header with value
/// `urgency=low` or `urgency=high`.
///
/// Returns other 4xx or 5xx responses on other errors.
#[post("/v1/client/add-version/{parent_version_id}")]
pub(crate) async fn service(
    req: HttpRequest,
    server_state: web::Data<Arc<ServerState>>,
    path: web::Path<VersionId>,
    mut payload: web::Payload,
) -> Result<HttpResponse> {
    let parent_version_id = path.into_inner();

    // check content-type
    if req.content_type() != HISTORY_SEGMENT_CONTENT_TYPE {
        return Err(error::ErrorBadRequest("Bad content-type"));
    }

    let client_id = client_id_header(&req)?;

    // read the body in its entirety
    let mut body = web::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        let chunk = chunk?;
        // limit max size of in-memory payload
        if (body.len() + chunk.len()) > MAX_SIZE {
            return Err(error::ErrorBadRequest("overflow"));
        }
        body.extend_from_slice(&chunk);
    }

    if body.is_empty() {
        return Err(error::ErrorBadRequest("Empty body"));
    }

    loop {
        return match server_state
            .server
            .add_version(client_id, parent_version_id, body.to_vec())
        {
            Ok((AddVersionResult::Ok(version_id), snap_urgency)) => {
                let mut rb = HttpResponse::Ok();
                rb.append_header((VERSION_ID_HEADER, version_id.to_string()));
                match snap_urgency {
                    SnapshotUrgency::None => {}
                    SnapshotUrgency::Low => {
                        rb.append_header((SNAPSHOT_REQUEST_HEADER, "urgency=low"));
                    }
                    SnapshotUrgency::High => {
                        rb.append_header((SNAPSHOT_REQUEST_HEADER, "urgency=high"));
                    }
                };
                Ok(rb.finish())
            }
            Ok((AddVersionResult::ExpectedParentVersion(parent_version_id), _)) => {
                let mut rb = HttpResponse::Conflict();
                rb.append_header((PARENT_VERSION_ID_HEADER, parent_version_id.to_string()));
                Ok(rb.finish())
            }
            Err(ServerError::NoSuchClient) => {
                // Create a new client and repeat the `add_version` call.
                let mut txn = server_state.server.txn().map_err(server_error_to_actix)?;
                txn.new_client(client_id, NIL_VERSION_ID)
                    .map_err(failure_to_ise)?;
                txn.commit().map_err(failure_to_ise)?;
                continue;
            }
            Err(e) => Err(server_error_to_actix(e)),
        };
    }
}

#[cfg(test)]
mod test {
    use crate::api::CLIENT_ID_HEADER;
    use crate::WebServer;
    use actix_web::{http::StatusCode, test, App};
    use pretty_assertions::assert_eq;
    use taskchampion_sync_server_core::{InMemoryStorage, Storage};
    use uuid::Uuid;

    #[actix_rt::test]
    async fn test_success() {
        let client_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();

        // set up the storage contents..
        {
            let mut txn = storage.txn().unwrap();
            txn.new_client(client_id, Uuid::nil()).unwrap();
        }

        let server = WebServer::new(Default::default(), storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = format!("/v1/client/add-version/{}", parent_version_id);
        let req = test::TestRequest::post()
            .uri(&uri)
            .append_header((
                "Content-Type",
                "application/vnd.taskchampion.history-segment",
            ))
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .set_payload(b"abcd".to_vec())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // the returned version ID is random, but let's check that it's not
        // the passed parent version ID, at least
        let new_version_id = resp.headers().get("X-Version-Id").unwrap();
        assert!(new_version_id != &version_id.to_string());

        // Shapshot should be requested, since there is no existing snapshot
        let snapshot_request = resp.headers().get("X-Snapshot-Request").unwrap();
        assert_eq!(snapshot_request, "urgency=high");

        assert_eq!(resp.headers().get("X-Parent-Version-Id"), None);
    }

    #[actix_rt::test]
    async fn test_auto_add_client() {
        let client_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let server = WebServer::new(Default::default(), InMemoryStorage::new());
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = format!("/v1/client/add-version/{}", parent_version_id);
        let req = test::TestRequest::post()
            .uri(&uri)
            .append_header((
                "Content-Type",
                "application/vnd.taskchampion.history-segment",
            ))
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .set_payload(b"abcd".to_vec())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // the returned version ID is random, but let's check that it's not
        // the passed parent version ID, at least
        let new_version_id = resp.headers().get("X-Version-Id").unwrap();
        let new_version_id = Uuid::parse_str(new_version_id.to_str().unwrap()).unwrap();
        assert!(new_version_id != version_id);

        // Shapshot should be requested, since there is no existing snapshot
        let snapshot_request = resp.headers().get("X-Snapshot-Request").unwrap();
        assert_eq!(snapshot_request, "urgency=high");

        assert_eq!(resp.headers().get("X-Parent-Version-Id"), None);

        // Check that the client really was created
        {
            let mut txn = server.server_state.server.txn().unwrap();
            let client = txn.get_client(client_id).unwrap().unwrap();
            assert_eq!(client.latest_version_id, new_version_id);
            assert_eq!(client.snapshot, None);
        }
    }

    #[actix_rt::test]
    async fn test_conflict() {
        let client_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();

        // set up the storage contents..
        {
            let mut txn = storage.txn().unwrap();
            txn.new_client(client_id, version_id).unwrap();
        }

        let server = WebServer::new(Default::default(), storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = format!("/v1/client/add-version/{}", parent_version_id);
        let req = test::TestRequest::post()
            .uri(&uri)
            .append_header((
                "Content-Type",
                "application/vnd.taskchampion.history-segment",
            ))
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .set_payload(b"abcd".to_vec())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        assert_eq!(resp.headers().get("X-Version-Id"), None);
        assert_eq!(
            resp.headers().get("X-Parent-Version-Id").unwrap(),
            &version_id.to_string()
        );
    }

    #[actix_rt::test]
    async fn test_bad_content_type() {
        let client_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();
        let server = WebServer::new(Default::default(), storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = format!("/v1/client/add-version/{}", parent_version_id);
        let req = test::TestRequest::post()
            .uri(&uri)
            .append_header(("Content-Type", "not/correct"))
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .set_payload(b"abcd".to_vec())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_rt::test]
    async fn test_empty_body() {
        let client_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();
        let server = WebServer::new(Default::default(), storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = format!("/v1/client/add-version/{}", parent_version_id);
        let req = test::TestRequest::post()
            .uri(&uri)
            .append_header((
                "Content-Type",
                "application/vnd.taskchampion.history-segment",
            ))
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
