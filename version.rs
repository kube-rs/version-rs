use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    routing::get,
    AddExtensionLayer, Json, Router,
};
use futures::StreamExt;
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    runtime::{
        reflector::{self, reflector, ObjectRef, Store},
        utils::try_flatten_touched,
        watcher,
    },
    Api, Client, ResourceExt,
};
use std::net::SocketAddr;
use tokio::signal::unix::{signal, SignalKind};
use tower_http::trace::TraceLayer;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn};

type Result<T> = std::result::Result<T, anyhow::Error>;

#[derive(serde::Serialize, Clone)]
pub struct Entry {
    container: String,
    name: String,
    namespace: String,
    version: String,
}
impl TryFrom<&Deployment> for Entry {
    type Error = anyhow::Error;

    fn try_from(d: &Deployment) -> Result<Self> {
        let name = d.name();
        let namespace = d.namespace().clone().unwrap();
        let spec = d.spec.as_ref().unwrap().template.spec.as_ref().unwrap();
        if let Some(img) = spec.containers[0].image.clone() {
            // main container only
            let split: Vec<_> = img.splitn(2, ':').collect();
            let (container, version) = match *split.as_slice() {
                [c, v] => (c.to_string(), v.to_string()),
                [c] => (c.to_string(), "latest".to_string()),
                _ => anyhow::bail!("missing container.image on {}", name),
            };
            return Ok(Entry { name, namespace, container, version });
        }
        Err(anyhow::anyhow!("Failed to parse deployment {}", name))
    }
}

// GET /versions
#[instrument(skip(store))]
async fn get_versions(store: Extension<Store<Deployment>>) -> Json<Vec<Entry>> {
    let state: Vec<Entry> = store
        .state()
        .into_iter()
        .filter_map(|d| Entry::try_from(d.as_ref()).ok())
        .collect();
    Json(state)
}

// GET /versions/<namespace>/<name>
#[instrument(skip(store))]
async fn get_version(
    store: Extension<Store<Deployment>>,
    Path((namespace, name)): Path<(String, String)>,
) -> std::result::Result<Json<Entry>, (StatusCode, &'static str)> {
    let key = ObjectRef::new(&name).within(&namespace);
    if let Some(d) = store.get(&key) {
        if let Ok(e) = Entry::try_from(d.as_ref()) {
            return Ok(Json(e));
        }
    }
    Err((StatusCode::NOT_FOUND, "not found"))
}

// GET /health
async fn health() -> (StatusCode, Json<&'static str>) {
    (StatusCode::OK, Json("healthy"))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let client = Client::try_default().await?;
    let api: Api<Deployment> = Api::default_namespaced(client);

    let store = reflector::store::Writer::<Deployment>::default();
    let reader = store.as_reader(); // queriable state for Axum
    let rf = reflector(store, watcher(api, Default::default()));
    // need to run/drain the reflector:
    let drainer = try_flatten_touched(rf).for_each(|_o| futures::future::ready(()));

    let app = Router::new()
        .route("/versions", get(get_versions))
        .route("/versions/:namespace/:name", get(get_version))
        .layer(AddExtensionLayer::new(reader.clone()))
        .layer(TraceLayer::new_for_http())
        // Reminder: routes added *after* TraceLayer are not subject to its logging behavior
        .route("/health", get(health));

    let mut shutdown = signal(SignalKind::terminate())?;
    let server = axum::Server::bind(&SocketAddr::from(([0, 0, 0, 0], 8000)))
        .serve(app.into_make_service())
        .with_graceful_shutdown(async move {
            shutdown.recv().await;
        });

    tokio::select! {
        _ = drainer => warn!("reflector drained"),
        _ = server => info!("axum exited"),
    }
    Ok(())
}
