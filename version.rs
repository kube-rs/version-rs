use actix_web::{get, middleware, web, web::Data, App, HttpRequest, HttpResponse, HttpServer, Responder};
use futures::{future::FutureExt, pin_mut, select};
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    api::{Api, Meta},
    runtime::Reflector,
    Client,
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
            if img.contains(":") {
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
async fn get_versions(rf: Data<Reflector<Deployment>>) -> impl Responder {
    let state: Vec<Entry> = rf
        .state()
        .await
        .unwrap()
        .into_iter()
        .filter_map(|d| Entry::try_from(d).ok())
        .collect();
    HttpResponse::Ok().json(state)
}
#[get("/versions/{name}")]
async fn get_version(rf: Data<Reflector<Deployment>>, name: web::Path<String>) -> impl Responder {
    if let Some(d) = rf.get(&name).await.unwrap() {
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
    env::set_var("RUST_LOG", "info,kube=debug");
    env_logger::init();
    let client = Client::try_default().await.expect("create client");
    let namespace = env::var("NAMESPACE").unwrap_or("default".into());
    let api: Api<Deployment> = Api::namespaced(client, &namespace);
    let rf = Reflector::new(api);

    let rf2 = rf.clone(); // clone for actix
    let server_fut = HttpServer::new(move || {
            App::new()
                .data(rf2.clone())
                .wrap(middleware::Logger::default()) //.exclude("/health"))
                .service(get_versions)
                .service(get_version)
                .service(health)
            })
        .bind("0.0.0.0:8000")
        .expect("bind to 0.0.0.0:8000")
        .shutdown_timeout(5)
        .run()
        .fuse();
    let reflector_fut = rf.run().fuse();

    // Ensure both runtimes are alive
    pin_mut!(server_fut, reflector_fut);
    select! {
        server_res = server_fut => { server_res }
        reflector_res = reflector_fut => { Ok(reflector_res.unwrap()) }
    }
}
