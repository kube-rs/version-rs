use kube_runtime::reflector::store::Writer;
use kube_runtime::utils::try_flatten_applied;
use kube_runtime::watcher;
use kube_runtime::reflector::Store;
use actix_web::{get, middleware, web, web::Data, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_prom::PrometheusMetrics;
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    api::{Api, Meta, ListParams},
    Client,
};
use kube_runtime::reflector;
use kube_runtime::reflector::ObjectRef;
use serde::Serialize;
use std::{convert::TryFrom, env};
use futures::{StreamExt, TryStreamExt};
use tokio::stream::Stream;

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
    let state: Vec<Entry> = store.iter()
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

fn spawn_periodic_reader(writer: Writer<Deployment>, api: Api<Deployment>) {
    tokio::spawn(async move {
        let watcher = watcher(api, ListParams::default());
        let rf = reflector(writer, watcher);
        let mut rfa = try_flatten_applied(rf).boxed();
        while let Some(o) = rfa.try_next().await.unwrap() {
            println!("Applied {}", Meta::name(&o));
        }
    });
}


#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env::set_var("RUST_LOG", "info,kube=debug");
    env_logger::init();
    let client = Client::try_default().await.expect("create client");
    let namespace = env::var("NAMESPACE").unwrap_or("default".into());
    let api: Api<Deployment> = Api::namespaced(client, &namespace);

    let store = reflector::store::Writer::<Deployment>::default();
    let reader = store.as_reader();

    // Keep track of applied events in a task
    spawn_periodic_reader(store, api);

    let prometheus = PrometheusMetrics::new("api", Some("/metrics"), None);

    let _server = HttpServer::new(move || {
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
        .run()
        .await?;
    Ok(())

}
