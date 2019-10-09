use std::fs::create_dir;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use futures::future;
use futures::TryFutureExt;
use parking_lot::RwLock;
use tracing::{debug, info, Subscriber};

use toshi::{shutdown, support};
use toshi::cluster::rpc_server::RpcServer;
use toshi::commit::watcher;
use toshi::index::IndexCatalog;
use toshi::router::router_with_catalog;
use toshi::settings::{HEADER, RPC_HEADER, Settings};

fn get_subscriber() -> impl Subscriber {
    tracing_fmt::FmtSubscriber::builder()
        .with_ansi(true)
        .finish()
}

#[tokio::main]
pub async fn main() -> Result<(), ()> {
    let settings = support::settings();

    std::env::set_var("RUST_LOG", &settings.log_level);
    let sub = get_subscriber();
    tracing::subscriber::set_global_default(sub).expect("Unable to set default Subscriber");

    debug!("{:?}", &settings);

    if !Path::new(&settings.path).exists() {
        info!("Base data path {} does not exist, creating it...", settings.path);
        create_dir(settings.path.clone()).expect("Unable to create data directory");
    }

    let index_catalog = {
        let path = PathBuf::from(settings.path.clone());
        let index_catalog = match IndexCatalog::new(path, settings.clone()) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error creating IndexCatalog from path {} - {}", settings.path, e);
                std::process::exit(1);
            }
        };

        Arc::new(RwLock::new(index_catalog))
    };
    let shutdown = async { shutdown::shutdown().await; };
    futures::pin_mut!(shutdown);

    if settings.experimental_features.master {
        let server = async { run_master(Arc::clone(&index_catalog), &settings).await; };
        futures::pin_mut!(server);
        future::select(server, shutdown).await;
    } else {
        let server = async { run_data(Arc::clone(&index_catalog), &settings).await; };
        futures::pin_mut!(server);
        future::select(server, shutdown).await;
    }


    Ok(())
}

async fn run_data(catalog: Arc<RwLock<IndexCatalog>>, settings: &Settings) -> Result<(), ()> {
    let lock = Arc::new(AtomicBool::new(false));
    let commit_watcher = watcher(Arc::clone(&catalog), settings.auto_commit_duration, Arc::clone(&lock));
    let addr: IpAddr = settings
        .host
        .parse()
        .unwrap_or_else(|_| panic!("Invalid ip address: {}", &settings.host));
    let settings = settings.clone();
    let bind: SocketAddr = SocketAddr::new(addr, settings.port);

    println!("{}", RPC_HEADER);
    info!("I am a data node...Binding to: {}", addr);
    tokio::spawn(commit_watcher);

    RpcServer::serve(bind, catalog).map_err(|_| ()).await
}

async fn run_master(catalog: Arc<RwLock<IndexCatalog>>, settings: &Settings) -> Result<(), ()> {
    let bulk_lock = Arc::new(AtomicBool::new(false));
    let commit_watcher = watcher(Arc::clone(&catalog), settings.auto_commit_duration, Arc::clone(&bulk_lock));
    let addr: IpAddr = settings
        .host
        .parse()
        .unwrap_or_else(|_| panic!("Invalid ip address: {}", &settings.host));
    let bind: SocketAddr = SocketAddr::new(addr, settings.port);

    println!("{}", HEADER);

    if settings.experimental {
        let settings = settings.clone();
        let nodes = settings.experimental_features.nodes.clone();

        tokio::spawn(commit_watcher);
        if !nodes.is_empty() {
//            let update_read = Arc::clone(&catalog);
//            let update_lock = update_read.read();

//            tokio::spawn(update_lock.update_remote_indexes().map_err(|_| ()));
        }

        router_with_catalog(&bind, Arc::clone(&catalog), Arc::clone(&bulk_lock)).map_err(|_| ()).await
    } else {
        let watcher_clone = Arc::clone(&bulk_lock);
        tokio::spawn(commit_watcher);
        router_with_catalog(&bind, Arc::clone(&catalog), watcher_clone).map_err(|_| ()).await
    }
}
