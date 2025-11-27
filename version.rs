use axum::extract::{Path, State};
use axum::{http::StatusCode, response::IntoResponse, routing, Json, Router};
use futures::{future, StreamExt};
use k8s_openapi::api::apps::v1::Deployment;
use kube::runtime::{reflector, watcher, WatchStreamExt};
use kube::{Api, Client, ResourceExt};
use tracing::{debug, warn};

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

// - GET /versions/:namespace/:name
#[derive(serde::Deserialize, Debug)]
struct EntryPath {
    name: String,
    namespace: String,
}
async fn get_version(State(store): State<Cache>, Path(p): Path<EntryPath>) -> impl IntoResponse {
    let key = reflector::ObjectRef::new(&p.name).within(&p.namespace);
    if let Some(Some(e)) = store.get(&key).map(|d| deployment_to_entry(&d)) {
        return Ok(Json(e));
    }
    Err((StatusCode::NOT_FOUND, "not found"))
}

// - GET /versions
async fn get_versions(State(store): State<Cache>) -> Json<Vec<Entry>> {
    let data = store.state().iter().filter_map(|d| deployment_to_entry(d)).collect();
    Json(data)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let client = Client::try_default().await?;
    let api: Api<Deployment> = Api::all(client);

    let (reader, writer) = reflector::store();
    let watch = reflector(writer, watcher(api, Default::default()))
        .default_backoff()
        .touched_objects()
        .for_each(|r| {
            future::ready(match r {
                Ok(o) => debug!("Saw {} in {}", o.name_any(), o.namespace().unwrap()),
                Err(e) => warn!("watcher error: {e}"),
            })
        });
    tokio::spawn(watch); // poll forever

    let app = Router::new()
        .route("/versions", routing::get(get_versions))
        .route("/versions/{namespace}/{name}", routing::get(get_version))
        .with_state(reader) // routes can read from the reflector store
        .layer(tower_http::trace::TraceLayer::new_for_http())
        // NB: routes added after TraceLayer are not traced
        .route("/health", routing::get(|| async { "up" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
