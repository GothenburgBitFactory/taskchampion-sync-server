use crate::api::{server_error_to_actix, ServerState, SNAPSHOT_CONTENT_TYPE, VERSION_ID_HEADER};
use actix_web::{get, web, HttpRequest, HttpResponse, Result};
use std::sync::Arc;

/// Get the latest version ID.
#[get("/v1/client/latest-version-id")]
pub(crate) async fn service(
    req: HttpRequest,
    server_state: web::Data<Arc<ServerState>>,
) -> Result<HttpResponse> {
    let client_id = server_state.client_id_header(&req)?;

    let latest_version_id = server_state
        .server
        .latest_version_id(client_id)
        .await
        .map_err(server_error_to_actix)?;
    Ok(HttpResponse::Ok()
        .append_header((VERSION_ID_HEADER, latest_version_id.to_string()))
        .body(()))
}

#[cfg(test)]
mod test {
    use crate::{
        api::CLIENT_ID_HEADER,
        web::{WebConfig, WebServer},
    };
    use actix_web::{http::StatusCode, test, App};
    use pretty_assertions::assert_eq;
    use taskchampion_sync_server_core::{InMemoryStorage, ServerConfig, Storage};
    use uuid::Uuid;

    #[actix_rt::test]
    async fn test_success() {
        let client_id = Uuid::new_v4();
        let latest_version_id = Uuid::new_v4();
        let storage = InMemoryStorage::new();

        // set up the storage contents..
        {
            let mut txn = storage.txn(client_id).await.unwrap();
            txn.new_client(latest_version_id).await.unwrap();
            txn.commit().await.unwrap();
        }

        let server = WebServer::new(ServerConfig::default(), WebConfig::default(), storage);
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let uri = "/v1/client/latest-version-id";
        let req = test::TestRequest::get()
            .uri(uri)
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        assert_eq!(
            resp.headers().get("X-Version-Id").unwrap(),
            &latest_version_id.to_string()
        );
    }
}
