use axum::{extract::Extension, http::StatusCode, response::IntoResponse, routing, Json, Router};
use axum_extra::routing::TypedPath;
use futures::{future, StreamExt};
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    runtime::{reflector, watcher, WatchStreamExt},
    Api, Client, ResourceExt,
};
use tracing::*;
type Result<T, E = anyhow::Error> = std::result::Result<T, E>;

#[derive(serde::Serialize, Clone)]
struct Entry {
    container: String,
    name: String,
    namespace: String,
    version: String,
}
fn deployment_to_entry(d: &Deployment) -> Option<Entry> {
    let name = d.name_any();
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

#[instrument(skip(store))]
async fn get_versions(store: Extension<reflector::Store<Deployment>>) -> Json<Vec<Entry>> {
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
async fn get_version(store: Extension<reflector::Store<Deployment>>, path: EntryPath) -> impl IntoResponse {
    let key = reflector::ObjectRef::new(&path.name).within(&path.namespace);
    if let Some(Some(e)) = store.get(&key).map(|d| deployment_to_entry(&d)) {
        return Ok(Json(e));
    }
    Err((StatusCode::NOT_FOUND, "not found"))
}

async fn health() -> impl IntoResponse {
    Json("healthy")
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::DEBUG).init();
    let client = Client::try_default().await?;
    let api: Api<Deployment> = Api::all(client);

    let (reader, writer) = reflector::store();
    // start and run the reflector
    let watch = reflector(writer, watcher(api, Default::default()))
        .touched_objects()
        .filter_map(|x| async move { Result::ok(x) })
        .for_each(|o| {
            debug!("Saw {} in {}", o.name_any(), o.namespace().unwrap());
            future::ready(())
        });

    let app = Router::new()
        .route("/versions", routing::get(get_versions))
        .route("/versions/:namespace/:name", routing::get(get_version))
        .layer(Extension(reader.clone()))
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
        _ = watch => warn!("reflector exited"),
        _ = server => info!("axum exited"),
    }
    Ok(())
}
