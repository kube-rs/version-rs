use axum::{extract::State, http::StatusCode, response::IntoResponse, routing, Json, Router};
use axum_extra::routing::TypedPath;
use futures::{future, StreamExt};
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    runtime::{reflector, watcher, WatchStreamExt},
    Api, Client, ResourceExt,
};
use tracing::{debug, info, instrument, warn, Level};

#[derive(serde::Serialize, Clone)]
struct Entry {
    container: String,
    name: String,
    namespace: String,
    version: String,
}
type Cache = reflector::Store<Deployment>;

fn deployment_to_entry(d: &Deployment) -> Option<Entry> {
    let name = d.name_any();
    let namespace = d.namespace()?;
    let tpl = d.spec.as_ref()?.template.spec.as_ref()?;
    let img = tpl.containers.get(0)?.image.as_ref()?;
    let splits = img.splitn(2, ':').collect::<Vec<_>>();
    let (container, version) = match *splits.as_slice() {
        [c, v] => (c.to_owned(), v.to_owned()),
        [c] => (c.to_owned(), "latest".to_owned()),
        _ => return None,
    };
    Some(Entry { name, namespace, container, version })
}

#[instrument(skip(store))]
async fn get_versions(State(store): State<Cache>) -> Json<Vec<Entry>> {
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
async fn get_version(State(store): State<Cache>, path: EntryPath) -> impl IntoResponse {
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
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::DEBUG).init();
    let client = Client::try_default().await?;
    let api: Api<Deployment> = Api::all(client);

    let (reader, writer) = reflector::store();
    let watch = reflector(writer, watcher(api, Default::default()))
        .default_backoff()
        .touched_objects()
        .filter_map(|x| async move { Result::ok(x) })
        .for_each(|o| {
            debug!("Saw {} in {}", o.name_any(), o.namespace().unwrap());
            future::ready(())
        });

    let app = Router::new()
        .route("/versions", routing::get(get_versions))
        .route("/versions/:namespace/:name", routing::get(get_version))
        .with_state(reader)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        // NB: routes added after TraceLayer are not traced
        .route("/health", routing::get(health));

    let server = axum::Server::bind(&std::net::SocketAddr::from(([0, 0, 0, 0], 8000)))
        .serve(app.into_make_service())
        .with_graceful_shutdown(elegant_departure::tokio::depart().on_ctrl_c());

    // poll both axum server and kube watch to keep them moving forward
    // axum will always exit gracefully first, because watch runs forever
    tokio::select! {
        _ = watch => warn!("watch exited"),
       _ = server => info!("axum exited"),
    };
    Ok(())
}
