#[macro_use] extern crate log;
#[macro_use] extern crate serde_derive;
use actix_web::{
    web::{Data},
    HttpRequest, HttpResponse, middleware
};
use actix_web::{get, App, HttpServer, Responder};

use kube::{
    client::APIClient,
    config::Configuration,
    api::{Reflector, Object, Api},
};
use k8s_openapi::api::apps::v1::{DeploymentSpec, DeploymentStatus};
use std::sync::{Arc, RwLock};

type Result<T> = std::result::Result<T, anyhow::Error>;
type Deploy = Object<DeploymentSpec, DeploymentStatus>;

#[derive(Serialize, Clone)]
pub struct Entry {
    container: String,
    name: String,
    version: String,
}
impl Entry {
    fn from(d: Deploy) -> Option<Self> {
        let name = d.metadata.name;
        if let Some(ref img) = d.spec.template.spec.unwrap().containers[0].image {
            if img.contains(":") {
                let splits : Vec<_> = img.split(':').collect();
                let container = splits[0].to_string();
                let version = splits[1].to_string();
                return Some(Entry { container, name, version });
            }
        }
        warn!("Failed to parse deployment {}", name);
        None
    }
}

#[derive(Clone)]
pub struct Watcher {
    rf: Reflector<Deploy>,
    state: Arc<RwLock<Vec<Entry>>>,
    client: APIClient,
}

impl Watcher {
    pub async fn init(cfg: Configuration) -> Result<Watcher> {
        let c = Watcher::new(APIClient::new(cfg)).await?; // for app to read
        let c2 = c.clone(); // for looping task below
        tokio::spawn(async move {
            loop {
                if let Err(e) = c2.poll().await {
                    error!("Kube state failed to recover: {}", e);
                    std::process::exit(1); // kube will restart here
                }
            }
        });
        Ok(c)
    }
    async fn new(client: APIClient) -> Result<Self> {
        let namespace = std::env::var("NAMESPACE").unwrap_or("default".into());
        let resource = Api::v1Deployment(client.clone()).within(&namespace);
        let rf = Reflector::new(resource).init().await?;
        let state = Arc::new(RwLock::new(Watcher::read(&rf).await?));
        Ok(Watcher { rf, state, client })
    }
    async fn read(rf: &Reflector<Deploy>) -> Result<Vec<Entry>> {
        Ok(rf.state().await?.into_iter()
            .filter_map(Entry::from)
            .collect())
    }
    async fn poll(&self) -> Result<()> {
        self.rf.poll().await?; // update cache every re-sync
        *self.state.write().unwrap() = Watcher::read(&self.rf).await?;
        Ok(())
    }
    pub fn state(&self) -> Vec<Entry> {
        self.state.read().unwrap().clone()
    }
}

#[get("/")]
async fn versions(c: Data<Watcher>, _req: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json(c.state())
}
#[get("/health")]
async fn health(_: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    let cfg = if let Ok(c) = kube::config::incluster_config() {
        c
    } else {
        kube::config::load_kube_config().await.expect("Failed to load kube config")
    };
    let c = Watcher::init(cfg).await.expect("Failed to initialize watcher");

    //let prometheus = actix_web_prom::PrometheusMetrics::new("api", "/metrics");
    HttpServer::new(move || {
        App::new()
            .data(c.clone())
            .wrap(middleware::Logger::default().exclude("/health"))
            //.wrap(prometheus.clone())
            .service(versions)
            .service(health)
        })
        .bind("0.0.0.0:8000").expect("Can not bind to 0.0.0.0:8000")
        .shutdown_timeout(0)
        .start()
        .await
}
