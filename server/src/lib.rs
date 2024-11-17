#![deny(clippy::all)]

mod api;

use actix_web::{get, middleware, web, Responder};
use api::{api_scope, ServerState};
use std::sync::Arc;
use taskchampion_sync_server_core::{ServerConfig, Storage};

#[get("/")]
async fn index() -> impl Responder {
    format!("TaskChampion sync server v{}", env!("CARGO_PKG_VERSION"))
}

/// A Server represents a sync server.
#[derive(Clone)]
pub struct Server {
    server_state: Arc<ServerState>,
}

impl Server {
    /// Create a new sync server with the given storage implementation.
    pub fn new(config: ServerConfig, storage: Box<dyn Storage>) -> Self {
        Self {
            server_state: Arc::new(ServerState { config, storage }),
        }
    }

    /// Get an Actix-web service for this server.
    pub fn config(&self, cfg: &mut web::ServiceConfig) {
        cfg.service(
            web::scope("")
                .app_data(web::Data::new(self.server_state.clone()))
                .wrap(
                    middleware::DefaultHeaders::new().add(("Cache-Control", "no-store, max-age=0")),
                )
                .service(index)
                .service(api_scope()),
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use actix_web::{test, App};
    use pretty_assertions::assert_eq;
    use taskchampion_sync_server_core::InMemoryStorage;

    #[actix_rt::test]
    async fn test_cache_control() {
        let server = Server::new(Default::default(), Box::new(InMemoryStorage::new()));
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        assert_eq!(
            resp.headers().get("Cache-Control").unwrap(),
            &"no-store, max-age=0".to_string()
        )
    }
}
