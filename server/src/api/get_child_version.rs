use crate::api::{
    server_error_to_actix, ServerState, HISTORY_SEGMENT_CONTENT_TYPE, PARENT_VERSION_ID_HEADER,
    VERSION_ID_HEADER,
};
use actix_web::{error, get, web, HttpRequest, HttpResponse, Result};
use std::sync::Arc;
use taskchampion_sync_server_core::{GetVersionResult, ServerError, VersionId};

/// Get a child version.
///
/// On succcess, the response is the same sequence of bytes originally sent to the server,
/// with content-type `application/vnd.taskchampion.history-segment`.  The `X-Version-Id` and
/// `X-Parent-Version-Id` headers contain the corresponding values.
///
/// If no such child exists, returns a 404 with no content.
/// Returns other 4xx or 5xx responses on other errors.
#[get("/v1/client/get-child-version/{parent_version_id}")]
pub(crate) async fn service(
    req: HttpRequest,
    server_state: web::Data<Arc<ServerState>>,
    path: web::Path<VersionId>,
) -> Result<HttpResponse> {
    let parent_version_id = path.into_inner();
    let client_id = server_state.client_id_header(&req)?;

    return match server_state
        .server
        .get_child_version(client_id, parent_version_id)
    {
        Ok(GetVersionResult::Success {
            version_id,
            parent_version_id,
            history_segment,
        }) => Ok(HttpResponse::Ok()
            .content_type(HISTORY_SEGMENT_CONTENT_TYPE)
            .append_header((VERSION_ID_HEADER, version_id.to_string()))
            .append_header((PARENT_VERSION_ID_HEADER, parent_version_id.to_string()))
            .body(history_segment)),
        Ok(GetVersionResult::NotFound) => Err(error::ErrorNotFound("no such version")),
        Ok(GetVersionResult::Gone) => Err(error::ErrorGone("version has been deleted")),
        // Note that the HTTP client cannot differentiate `NotFound` and `NoSuchClient`, as both
        // are a 404 NOT FOUND response. In either case, the HTTP client will typically attempt
        // to add a new version, which may create the new client at the same time.
        Err(ServerError::NoSuchClient) => Err(error::ErrorNotFound("no such client")),
        Err(e) => Err(server_error_to_actix(e)),
    };
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
    async fn test_success() {
        let client_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();

        // set up the storage contents..
        {
            let mut txn = storage.txn(client_id).unwrap();
            txn.new_client(Uuid::new_v4()).unwrap();
            txn.add_version(version_id, parent_version_id, b"abcd".to_vec())
                .unwrap();
            txn.commit().unwrap();
        }

        let server = WebServer::new(Default::default(), None, storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = format!("/v1/client/get-child-version/{}", parent_version_id);
        let req = test::TestRequest::get()
            .uri(&uri)
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("X-Version-Id").unwrap(),
            &version_id.to_string()
        );
        assert_eq!(
            resp.headers().get("X-Parent-Version-Id").unwrap(),
            &parent_version_id.to_string()
        );
        assert_eq!(
            resp.headers().get("Content-Type").unwrap(),
            &"application/vnd.taskchampion.history-segment".to_string()
        );

        use actix_web::body::MessageBody;
        let bytes = resp.into_body().try_into_bytes().unwrap();
        assert_eq!(bytes.as_ref(), b"abcd");
    }

    #[actix_rt::test]
    async fn test_client_not_found() {
        let client_id = Uuid::new_v4();
        let parent_version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();
        let server = WebServer::new(Default::default(), None, storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = format!("/v1/client/get-child-version/{}", parent_version_id);
        let req = test::TestRequest::get()
            .uri(&uri)
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(resp.headers().get("X-Version-Id"), None);
        assert_eq!(resp.headers().get("X-Parent-Version-Id"), None);
    }

    #[actix_rt::test]
    async fn test_version_not_found_and_gone() {
        let client_id = Uuid::new_v4();
        let test_version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();

        // create the client and a single version.
        {
            let mut txn = storage.txn(client_id).unwrap();
            txn.new_client(Uuid::new_v4()).unwrap();
            txn.add_version(test_version_id, NIL_VERSION_ID, b"vers".to_vec())
                .unwrap();
            txn.commit().unwrap();
        }
        let server = WebServer::new(Default::default(), None, storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        // the child of the nil version is the added version
        let uri = format!("/v1/client/get-child-version/{}", NIL_VERSION_ID);
        let req = test::TestRequest::get()
            .uri(&uri)
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("X-Version-Id").unwrap(),
            &test_version_id.to_string(),
        );
        assert_eq!(
            resp.headers().get("X-Parent-Version-Id").unwrap(),
            &NIL_VERSION_ID.to_string(),
        );

        // the child of an unknown parent_version_id is GONE.
        let uri = format!("/v1/client/get-child-version/{}", Uuid::new_v4());
        let req = test::TestRequest::get()
            .uri(&uri)
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::GONE);
        assert_eq!(resp.headers().get("X-Version-Id"), None);
        assert_eq!(resp.headers().get("X-Parent-Version-Id"), None);

        // The child of the latest version is NOT_FOUND. The tests in crate::server test more
        // corner cases.
        let uri = format!("/v1/client/get-child-version/{}", test_version_id);
        let req = test::TestRequest::get()
            .uri(&uri)
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(resp.headers().get("X-Version-Id"), None);
        assert_eq!(resp.headers().get("X-Parent-Version-Id"), None);
    }
}
