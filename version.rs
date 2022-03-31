use axum::{extract::Extension, http::StatusCode, response::IntoResponse, routing, Json, Router};
use axum_extra::routing::{RouterExt, TypedPath};
use futures::{future, StreamExt};
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    runtime::{
        reflector::{self, reflector, ObjectRef, Store},
        utils::try_flatten_touched,
        watcher,
    },
    Api, Client, ResourceExt,
};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Level};

type Result<T, E = anyhow::Error> = std::result::Result<T, E>;

#[derive(serde::Serialize, Clone)]
struct Entry {
    container: String,
    name: String,
    namespace: String,
    version: String,
}
fn deployment_to_entry(d: &Deployment) -> Option<Entry> {
    let name = d.name();
    let namespace = d.namespace()?;
    let tpl = d.spec.as_ref()?.template.spec.as_ref()?;
    let img = tpl.containers.get(0)?.image.as_ref()?;
    let splits = img.splitn(2, ':').collect::<Vec<_>>();
    let (container, version) = match *splits.as_slice() {
        [c, v] => (c.to_string(), v.to_string()),
        [c] => (c.to_string(), "latest".to_string()),
        _ => return None,
    };
    Some(Entry { name, namespace, container, version })
}

type Cache = Store<Deployment>;

#[instrument(skip(store))]
async fn get_versions(store: Extension<Cache>) -> Json<Vec<Entry>> {
    let data = store.state().iter().filter_map(|d| deployment_to_entry(d)).collect();
    Json(data)
}

#[derive(TypedPath, serde::Deserialize, Debug)]
#[typed_path("/versions/:namespace/:name")]
struct EntryPath {
    name: String,
    namespace: String,
}

#[instrument(skip(store))]
async fn get_version(store: Extension<Cache>, path: EntryPath) -> impl IntoResponse {
    let key = ObjectRef::new(&path.name).within(&path.namespace);
    if let Some(d) = store.get(&key) {
        if let Some(e) = deployment_to_entry(&d) {
            return Ok(Json(e));
        }
    }
    Err((StatusCode::NOT_FOUND, "not found"))
}

// GET /health
async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json("healthy"))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::DEBUG).init();
    let client = Client::try_default().await?;
    let api: Api<Deployment> = Api::default_namespaced(client);

    let store = reflector::store::Writer::<Deployment>::default();
    let reader = store.as_reader(); // queriable state for Axum
    let rf = reflector(store, watcher(api, Default::default()));
    // need to run/drain the reflector - so utilize the for_each to log deployment watch events
    let drainer = try_flatten_touched(rf)
        .filter_map(|x| async move { Result::ok(x) })
        .for_each(|o| {
            debug!("Saw {:?}", o.name());
            future::ready(())
        });

    let app = Router::new()
        .route("/versions", routing::get(get_versions))
        .route("/versions/:namespace/:name", routing::get(get_version))
        .layer(Extension(reader.clone()))
        .typed_get(get_versions)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        // Reminder: routes added *after* TraceLayer are not subject to its logging behavior
        .route("/health", routing::get(health));

    use tokio::signal::unix as usig;
    let mut shutdown = usig::signal(usig::SignalKind::terminate())?;
    let server = axum::Server::bind(&std::net::SocketAddr::from(([0, 0, 0, 0], 8000)))
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
