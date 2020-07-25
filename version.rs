use actix_web::{get, middleware, web, web::Data, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_prom::PrometheusMetrics;
use futures::StreamExt;
#[allow(unused_imports)] use tracing::{debug, error, info, trace, warn};

use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    api::{Api, ListParams, Meta},
    Client,
};
use kube_runtime::{
    reflector,
    reflector::{ObjectRef, Store},
    utils::try_flatten_touched,
    watcher,
};
use serde::Serialize;
use std::{convert::TryFrom, env};

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
        .state()
        .into_iter()
        .filter_map(|d| Entry::try_from(d).ok())
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

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let client = Client::try_default().await.expect("create client");
    let namespace = env::var("NAMESPACE").unwrap_or("default".into());
    let api: Api<Deployment> = Api::namespaced(client, &namespace);

    let store = reflector::store::Writer::<Deployment>::default();
    let reader = store.as_reader(); // queriable state for actix
    let rf = reflector(store, watcher(api, ListParams::default()));
    // need to run/drain the reflector - so utilize the for_each to log toucheds
    let drainer = try_flatten_touched(rf)
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|o| {
            debug!("Touched {:?}", Meta::name(&o));
            futures::future::ready(())
        });

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
        _ = drainer => warn!("reflector drained"),
        _ = server => info!("actix exited"),
    }
    Ok(())
}
