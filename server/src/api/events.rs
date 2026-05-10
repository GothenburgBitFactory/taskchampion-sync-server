use crate::api::{ServerState, CLIENT_ID_HEADER};
use actix_web::{error, get, http::header, web, HttpRequest, HttpResponse, Result};
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    StreamExt,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use taskchampion_sync_server_core::{ClientId, VersionId};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeEvent {
    pub(crate) client_id: ClientId,
    pub(crate) version_id: VersionId,
}

#[derive(Clone, Default)]
pub(crate) struct ChangeNotifier {
    subscribers: Arc<Mutex<HashMap<ClientId, Vec<UnboundedSender<ChangeEvent>>>>>,
}

impl ChangeNotifier {
    pub(crate) fn subscribe(&self, client_id: ClientId) -> UnboundedReceiver<ChangeEvent> {
        let (tx, rx) = unbounded();
        self.subscribers
            .lock()
            .expect("change notifier mutex poisoned")
            .entry(client_id)
            .or_default()
            .push(tx);
        rx
    }

    pub(crate) fn notify(&self, client_id: ClientId, version_id: VersionId) {
        let event = ChangeEvent {
            client_id,
            version_id,
        };
        let mut subscribers = self
            .subscribers
            .lock()
            .expect("change notifier mutex poisoned");
        if let Some(client_subscribers) = subscribers.get_mut(&client_id) {
            client_subscribers
                .retain(|subscriber| subscriber.unbounded_send(event.clone()).is_ok());
        }
    }
}

#[get("/v1/client/events")]
pub(crate) async fn service(
    req: HttpRequest,
    server_state: web::Data<Arc<ServerState>>,
) -> Result<HttpResponse> {
    if !server_state.web_config.sync_events {
        return Err(error::ErrorNotFound("sync events are not enabled"));
    }

    let client_id = server_state.client_id_header(&req)?;
    let stream = server_state.changes.subscribe(client_id).map(|event| {
        let json = serde_json::to_string(&event).expect("change event serializes");
        Ok::<_, actix_web::Error>(web::Bytes::from(format!(
            "event: version\n\
             data: {json}\n\
             \n"
        )))
    });

    Ok(HttpResponse::Ok()
        .append_header((header::CONTENT_TYPE, "text/event-stream"))
        .append_header((header::CACHE_CONTROL, "no-store, max-age=0"))
        .append_header((header::CONNECTION, "keep-alive"))
        .append_header((CLIENT_ID_HEADER, client_id.to_string()))
        .streaming(stream))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::web::{WebConfig, WebServer};
    use actix_web::{http::StatusCode, test, App};
    use taskchampion_sync_server_core::{InMemoryStorage, ServerConfig};
    use uuid::Uuid;

    #[actix_rt::test]
    async fn notifier_delivers_events_for_matching_client() {
        let notifier = ChangeNotifier::default();
        let client_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let mut rx = notifier.subscribe(client_id);

        notifier.notify(client_id, version_id);
        let event = rx.next().await.unwrap();
        assert_eq!(event.client_id, client_id);
        assert_eq!(event.version_id, version_id);
    }

    #[actix_rt::test]
    async fn events_endpoint_uses_client_id_header() {
        let client_id = Uuid::new_v4();
        let server = WebServer::new(
            ServerConfig::default(),
            WebConfig {
                sync_events: true,
                ..WebConfig::default()
            },
            InMemoryStorage::new(),
        );
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri("/v1/client/events")
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
    }

    #[actix_rt::test]
    async fn events_endpoint_is_disabled_by_default() {
        let client_id = Uuid::new_v4();
        let server = WebServer::new(
            ServerConfig::default(),
            WebConfig::default(),
            InMemoryStorage::new(),
        );
        let app = App::new().configure(|sc| server.config(sc));
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri("/v1/client/events")
            .append_header((CLIENT_ID_HEADER, client_id.to_string()))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
