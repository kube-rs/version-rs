#[macro_use] extern crate log;
#[macro_use] extern crate serde_derive;
use actix_web::{
  web::{self, Data},
  App, HttpServer, HttpRequest, HttpResponse, middleware,
};
use kube::{
    client::APIClient,
    config::Configuration,
    api::{Reflector, Object, Api},
};
use k8s_openapi::api::apps::v1::{DeploymentSpec, DeploymentStatus};
use std::sync::{Arc, RwLock};

type Result<T> = std::result::Result<T, failure::Error>;
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
    pub fn init(cfg: Configuration) -> Result<Watcher> {
        let c = Watcher::new(APIClient::new(cfg))?; // for app to read
        let c2 = c.clone(); // for poll thread to write
        std::thread::spawn(move || {
            loop {
                let _ = c2.poll().map_err(|e| {
                    error!("Kube state failed to recover: {}", e);
                    std::process::exit(1); // kube will restart here
                });
            }
        });
        Ok(c)
    }
    fn new(client: APIClient) -> Result<Self> {
        let namespace = std::env::var("NAMESPACE").unwrap_or("default".into());
        let resource = Api::v1Deployment(client.clone()).within(&namespace);
        let rf = Reflector::new(resource).init()?;
        let state = Arc::new(RwLock::new(Vec::new()));
        Ok(Watcher { rf, state, client })
    }
    fn poll(&self) -> Result<()> {
        self.rf.poll()?;
        let state = self.rf.read()?.into_iter()
            .filter_map(|(_, d)| Entry::from(d))
            .collect();
        *self.state.write().unwrap() = state;
        Ok(())
    }
    pub fn state(&self) -> Vec<Entry> {
        self.state.read().unwrap().clone()
    }
}

fn versions(c: Data<Watcher>, _req: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().json(c.state())
}
fn health(_: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().json("healthy")
}

fn main() {
    env_logger::init();
    let cfg = kube::config::incluster_config().or_else(|_| {
        kube::config::load_kube_config()
    }).expect("Failed to load kube config");
    let c = Watcher::init(cfg).expect("Failed to initialize watcher");

    let sys = actix::System::new("version");
    let prometheus = actix_web_prom::PrometheusMetrics::new("api", "/metrics");
    HttpServer::new(move || {
        App::new()
            .data(c.clone())
            .wrap(middleware::Logger::default().exclude("/health"))
            .wrap(prometheus.clone())
            .service(web::resource("/").to(versions))
            .service(web::resource("/health").to(health))
        })
        .bind("0.0.0.0:8000").expect("Can not bind to 0.0.0.0:8000")
        .shutdown_timeout(0)
        .start();
    info!("Starting listening on 0.0.0.0:8000");
    let _ = sys.run();
}
