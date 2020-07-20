use actix_web::{get, middleware, web, web::Data, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_prom::PrometheusMetrics;
use futures::{StreamExt, TryStreamExt};
use futures_util::stream::LocalBoxStream;
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    api::{Api, ListParams, Meta},
    Client,
};
use kube_runtime::{
    reflector,
    reflector::{ObjectRef, Store},
    utils::try_flatten_applied,
    watcher,
};
use serde::Serialize;
use std::{
    convert::TryFrom,
    env,
    sync::{Arc, Mutex},
};

type Result<T> = std::result::Result<T, anyhow::Error>;

#[derive(Serialize, Clone)]
pub struct Entry {
    container: String,
    name: String,
    version: String,
}
impl TryFrom<Deployment> for Entry {
    type Error = anyhow::Error;

    fn try_from(d: Deployment) -> Result<Self> {
        let name = Meta::name(&d);
        if let Some(ref img) = d.spec.unwrap().template.spec.unwrap().containers[0].image {
            if img.contains(':') {
                let splits: Vec<_> = img.split(':').collect();
                let container = splits[0].to_string();
                let version = splits[1].to_string();
                return Ok(Entry {
                    container,
                    name,
                    version,
                });
            }
        }
        Err(anyhow::anyhow!("Failed to parse deployment {}", name))
    }
}

#[get("/versions")]
async fn get_versions(store: Data<Store<Deployment>>) -> impl Responder {
    let state: Vec<Entry> = store
        .iter()
        .filter_map(|eg| Entry::try_from(eg.value().clone()).ok())
        .collect();
    HttpResponse::Ok().json(state)
}
#[get("/versions/{name}")]
async fn get_version(store: Data<Store<Deployment>>, name: web::Path<String>) -> impl Responder {
    let namespace = env::var("NAMESPACE").unwrap_or("default".into());
    let key = ObjectRef::new(&name).within(&namespace);
    if let Some(d) = store.get(&key) {
        if let Ok(e) = Entry::try_from(d) {
            return HttpResponse::Ok().json(e);
        }
    }
    HttpResponse::NotFound().finish()
}
#[get("/health")]
async fn health(_: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

// This is awkward atm because our impl Stream is not Sync
type StreamItem = std::result::Result<Deployment, kube_runtime::watcher::Error>;
type DeployStream = LocalBoxStream<'static, StreamItem>;
async fn local_watcher(s: Arc<Mutex<DeployStream>>) -> std::result::Result<(), kube_runtime::watcher::Error> {
    let mut su = s.lock().unwrap();
    while let Some(o) = su.try_next().await? {
        println!("Applied {}", Meta::name(&o));
    }
    Ok(())
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env::set_var("RUST_LOG", "info,kube=debug");
    env_logger::init();
    let client = Client::try_default().await.expect("create client");
    let namespace = env::var("NAMESPACE").unwrap_or("default".into());
    let api: Api<Deployment> = Api::namespaced(client, &namespace);

    let store = reflector::store::Writer::<Deployment>::default();
    let reader = store.as_reader(); // queriable state for actix
    let rf = reflector(store, watcher(api, ListParams::default()));
    // stream that another thread will consume
    let rfa = Arc::new(Mutex::new(try_flatten_applied(rf).boxed_local()));

    let prometheus = PrometheusMetrics::new("api", Some("/metrics"), None);

    let server = HttpServer::new(move || {
        App::new()
            .data(reader.clone())
            .wrap(middleware::Logger::default().exclude("/health"))
            .wrap(prometheus.clone())
            .service(get_versions)
            .service(get_version)
            .service(health)
    })
    .bind("0.0.0.0:8000")
    .expect("bind to 0.0.0.0:8000")
    .shutdown_timeout(5)
    .run();
    tokio::select! {
        _ = local_watcher(rfa) => println!("logger done"),
        _ = server => println!("server done"),
    }
    Ok(())
}
