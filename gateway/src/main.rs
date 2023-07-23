use hyper::{
    body::to_bytes,
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, StatusCode,
};
use log::info;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

#[derive(Debug, Serialize, Deserialize)]
struct BotData {
    ip: String,
    version: String,
}

struct Bots {
    bots: Mutex<HashMap<String, BotData>>,
    max_id: Mutex<i32>, // Keep track of the maximum ID used so far
}

impl Bots {
    fn new() -> Self {
        Bots {
            bots: Mutex::new(HashMap::new()),
            max_id: Mutex::new(0),
        }
    }
}

async fn handle(bots: Arc<Bots>, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    info!("Received request: {:?}", req);

    let bytes = to_bytes(req).await?;
    let bot_data: BotData = serde_json::from_slice(&bytes).unwrap();

    let mut bots_lock = bots.bots.lock().unwrap();
    let mut id_lock = bots.max_id.lock().unwrap();
    *id_lock += 1; // Increment the ID for each new bot
    let id = id_lock.to_string(); // Convert to string for use as a HashMap key

    bots_lock.insert(id.clone(), bot_data);

    // Log the current list of bots using info!
    info!("Current list of bots: {:?}", *bots_lock);

    Ok(Response::new(Body::from(format!("Assigned ID: {}", id))))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::init();
    let addr = SocketAddr::from(([127, 0, 0, 1], 1111));
    let bots = Arc::new(Bots::new());
    let mut current_bot_id = 0;
    let make_service = make_service_fn(move |_| {
        let bots = bots.clone();
        async { Ok::<_, Infallible>(service_fn(move |req| handle(bots.clone(), req))) }
    });

    let server = Server::bind(&addr).serve(make_service);
    info!("Server is running on {}", addr);

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }

    Ok(())
}
