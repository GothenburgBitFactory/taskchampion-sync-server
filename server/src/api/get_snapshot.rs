use crate::api::{server_error_to_actix, ServerState, SNAPSHOT_CONTENT_TYPE, VERSION_ID_HEADER};
use actix_web::{error, get, web, HttpRequest, HttpResponse, Result};
use std::sync::Arc;

/// Get a snapshot.
///
/// If a snapshot for this client exists, it is returned with content-type
/// `application/vnd.taskchampion.snapshot`.  The `X-Version-Id` header contains the version of the
/// snapshot.
///
/// If no snapshot exists, returns a 404 with no content.  Returns other 4xx or 5xx responses on
/// other errors.
#[get("/v1/client/snapshot")]
pub(crate) async fn service(
    req: HttpRequest,
    server_state: web::Data<Arc<ServerState>>,
) -> Result<HttpResponse> {
    let client_id = server_state.client_id_header(&req)?;

    if let Some((version_id, data)) = server_state
        .server
        .get_snapshot(client_id)
        .map_err(server_error_to_actix)?
    {
        Ok(HttpResponse::Ok()
            .content_type(SNAPSHOT_CONTENT_TYPE)
            .append_header((VERSION_ID_HEADER, version_id.to_string()))
            .body(data))
    } else {
        Err(error::ErrorNotFound("no snapshot"))
    }
}

#[cfg(test)]
mod test {
    use crate::api::CLIENT_ID_HEADER;
    use crate::WebServer;
    use actix_web::{http::StatusCode, test, App};
    use chrono::{TimeZone, Utc};
    use pretty_assertions::assert_eq;
    use taskchampion_sync_server_core::{InMemoryStorage, Snapshot, Storage};
    use uuid::Uuid;

    #[actix_rt::test]
    async fn test_not_found() {
        let client_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();

        // set up the storage contents..
        {
            let mut txn = storage.txn().unwrap();
            txn.new_client(client_id, Uuid::new_v4()).unwrap();
        }

        let server = WebServer::new(Default::default(), None, storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = "/v1/client/snapshot";
        let req = test::TestRequest::get()
            .uri(uri)
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[actix_rt::test]
    async fn test_success() {
        let client_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let snapshot_data = vec![1, 2, 3, 4];
        let storage = InMemoryStorage::new();

        // set up the storage contents..
        {
            let mut txn = storage.txn().unwrap();
            txn.new_client(client_id, Uuid::new_v4()).unwrap();
            txn.set_snapshot(
                client_id,
                Snapshot {
                    version_id,
                    versions_since: 3,
                    timestamp: Utc.with_ymd_and_hms(2001, 9, 9, 1, 46, 40).unwrap(),
                },
                snapshot_data.clone(),
            )
            .unwrap();
        }

        let server = WebServer::new(Default::default(), None, storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = "/v1/client/snapshot";
        let req = test::TestRequest::get()
            .uri(uri)
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        use actix_web::body::MessageBody;
        let bytes = resp.into_body().try_into_bytes().unwrap();
        assert_eq!(bytes.as_ref(), snapshot_data);
    }
}
