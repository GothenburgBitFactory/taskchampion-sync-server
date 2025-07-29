use crate::api::{api_scope, ServerState};
use actix_web::{
    dev::ServiceResponse,
    get,
    http::StatusCode,
    middleware,
    middleware::{ErrorHandlerResponse, ErrorHandlers, Logger},
    web, App, HttpServer, Responder,
};
use std::{collections::HashSet, sync::Arc};
use taskchampion_sync_server_core::{Server, ServerConfig, Storage};
use uuid::Uuid;

fn print_error<B>(res: ServiceResponse<B>) -> actix_web::Result<ErrorHandlerResponse<B>> {
    if let Some(err) = res.response().error() {
        log::error!("Internal Server Error caused by:\n{err:?}");
    }
    Ok(ErrorHandlerResponse::Response(res.map_into_left_body()))
}

/// Configuration for WebServer (as distinct from [`ServerConfig`]).
pub struct WebConfig {
    pub client_id_allowlist: Option<HashSet<Uuid>>,
    pub create_clients: bool,
    pub listen_addresses: Vec<String>,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            client_id_allowlist: Default::default(),
            create_clients: true,
            listen_addresses: vec![],
        }
    }
}

#[get("/")]
async fn index() -> impl Responder {
    format!("TaskChampion sync server v{}", env!("CARGO_PKG_VERSION"))
}

/// A Server represents a sync server.
#[derive(Clone)]
pub struct WebServer {
    pub(crate) server_state: Arc<ServerState>,
}

impl WebServer {
    /// Create a new sync server with the given storage implementation.
    pub fn new<ST: Storage + 'static>(
        config: ServerConfig,
        web_config: WebConfig,
        storage: ST,
    ) -> Self {
        Self {
            server_state: Arc::new(ServerState {
                server: Server::new(config, storage),
                web_config,
            }),
        }
    }

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

    pub async fn run(self) -> anyhow::Result<()> {
        let listen_addresses = self.server_state.web_config.listen_addresses.clone();
        let mut http_server = HttpServer::new(move || {
            App::new()
                .wrap(ErrorHandlers::new().handler(StatusCode::INTERNAL_SERVER_ERROR, print_error))
                .wrap(Logger::default())
                .configure(|cfg| self.config(cfg))
        });
        for listen_address in listen_addresses {
            log::info!("Serving on {listen_address}");
            http_server = http_server.bind(listen_address)?
        }
        http_server.run().await?;
        Ok(())
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
        let server = WebServer::new(
            ServerConfig::default(),
            WebConfig::default(),
            InMemoryStorage::new(),
        );
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
