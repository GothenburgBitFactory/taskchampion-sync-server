use crate::api::{server_error_to_actix, ServerState, SNAPSHOT_CONTENT_TYPE};
use actix_web::{error, post, web, HttpMessage, HttpRequest, HttpResponse, Result};
use futures::StreamExt;
use std::sync::Arc;
use taskchampion_sync_server_core::VersionId;

/// Max snapshot size: 100MB
const MAX_SIZE: usize = 100 * 1024 * 1024;

/// Add a new snapshot, after checking prerequisites.  The snapshot should be transmitted in the
/// request entity body and must have content-type `application/vnd.taskchampion.snapshot`.  The
/// content can be encoded in any of the formats supported by actix-web.
///
/// On success, the response is a 200 OK. Even in a 200 OK, the snapshot may not appear in a
/// subsequent `GetSnapshot` call.
///
/// Returns other 4xx or 5xx responses on other errors.
#[post("/v1/client/add-snapshot/{version_id}")]
pub(crate) async fn service(
    req: HttpRequest,
    server_state: web::Data<Arc<ServerState>>,
    path: web::Path<VersionId>,
    mut payload: web::Payload,
) -> Result<HttpResponse> {
    let version_id = path.into_inner();

    // check content-type
    if req.content_type() != SNAPSHOT_CONTENT_TYPE {
        return Err(error::ErrorBadRequest("Bad content-type"));
    }

    let client_id = server_state.client_id_header(&req)?;

    // read the body in its entirety
    let mut body = web::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        let chunk = chunk?;
        // limit max size of in-memory payload
        if (body.len() + chunk.len()) > MAX_SIZE {
            return Err(error::ErrorBadRequest("Snapshot over maximum allowed size"));
        }
        body.extend_from_slice(&chunk);
    }

    if body.is_empty() {
        return Err(error::ErrorBadRequest("No snapshot supplied"));
    }

    server_state
        .server
        .add_snapshot(client_id, version_id, body.to_vec())
        .map_err(server_error_to_actix)?;
    Ok(HttpResponse::Ok().body(""))
}

#[cfg(test)]
mod test {
    use crate::api::CLIENT_ID_HEADER;
    use crate::WebServer;
    use actix_web::{http::StatusCode, test, App};
    use pretty_assertions::assert_eq;
    use taskchampion_sync_server_core::{InMemoryStorage, Storage, NIL_VERSION_ID};
    use uuid::Uuid;

    #[actix_rt::test]
    async fn test_success() -> anyhow::Result<()> {
        let client_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();

        // set up the storage contents..
        {
            let mut txn = storage.txn(client_id).unwrap();
            txn.new_client(version_id).unwrap();
            txn.add_version(version_id, NIL_VERSION_ID, vec![])?;
            txn.commit()?;
        }

        let server = WebServer::new(Default::default(), None, true, storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = format!("/v1/client/add-snapshot/{version_id}");
        let req = test::TestRequest::post()
            .uri(&uri)
            .insert_header(("Content-Type", "application/vnd.taskchampion.snapshot"))
            .insert_header((CLIENT_ID_HEADER, client_id.to_string()))
            .set_payload(b"abcd".to_vec())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // read back that snapshot
        let uri = "/v1/client/snapshot";
        let req = test::TestRequest::get()
            .uri(uri)
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        use actix_web::body::MessageBody;
        let bytes = resp.into_body().try_into_bytes().unwrap();
        assert_eq!(bytes.as_ref(), b"abcd");

        Ok(())
    }

    #[actix_rt::test]
    async fn test_not_added_200() {
        let client_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();

        // set up the storage contents..
        {
            let mut txn = storage.txn(client_id).unwrap();
            txn.new_client(NIL_VERSION_ID).unwrap();
            txn.commit().unwrap();
        }

        let server = WebServer::new(Default::default(), None, true, storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        // add a snapshot for a nonexistent version
        let uri = format!("/v1/client/add-snapshot/{version_id}");
        let req = test::TestRequest::post()
            .uri(&uri)
            .append_header(("Content-Type", "application/vnd.taskchampion.snapshot"))
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .set_payload(b"abcd".to_vec())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // read back, seeing no snapshot
        let uri = "/v1/client/snapshot";
        let req = test::TestRequest::get()
            .uri(uri)
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[actix_rt::test]
    async fn test_bad_content_type() {
        let client_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();
        let server = WebServer::new(Default::default(), None, true, storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = format!("/v1/client/add-snapshot/{version_id}");
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
        let version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();
        let server = WebServer::new(Default::default(), None, true, storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = format!("/v1/client/add-snapshot/{version_id}");
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
