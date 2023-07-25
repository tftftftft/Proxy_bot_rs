use hyper::{
    body::{to_bytes, Bytes},
    client::Client,
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, Uri,
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tokio::time::interval;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct BotData {
    ip: String,
    version: String,
    bot_port: u16,
    uuid: String,
}

#[derive(Debug)]
struct ServerBotData {
    bot_data: BotData,
    assigned_port: u16,
    last_communication: SystemTime,
}

struct Bots {
    bots: Mutex<HashMap<u16, ServerBotData>>,
    max_id: Mutex<u16>,
}

impl Bots {
    fn new() -> Self {
        Bots {
            bots: Mutex::new(HashMap::new()),
            max_id: Mutex::new(0),
        }
    }

    fn bot_exists(&self, bot_data: &BotData) -> bool {
        let bots_lock = self.bots.lock().unwrap();
        bots_lock.values().any(|v| v.bot_data.uuid == bot_data.uuid)
    }

    fn assign_bot_id(&self) -> u16 {
        let mut id_lock = self.max_id.lock().unwrap();
        *id_lock += 1;
        *id_lock
    }
}

async fn handle_registration(
    bots: Arc<Bots>,
    req: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
    info!("Received registration request: {:?}", req.body());

    let bytes = to_bytes(req.into_body()).await?;
    let bot_data: BotData = serde_json::from_slice(&bytes).unwrap();

    if bots.bot_exists(&bot_data) {
        return Ok(Response::new(Body::from(
            "Bot with this UUID already exists.",
        )));
    }

    let id = bots.assign_bot_id();
    let assigned_port = 5000 + id;
    spawn_bot_server(bot_data.clone(), assigned_port, bots.clone());

    let mut bots_lock = bots.bots.lock().unwrap();
    bots_lock.insert(
        id,
        ServerBotData {
            bot_data: bot_data,
            assigned_port: assigned_port,
            last_communication: SystemTime::now(),
        },
    );

    let response_body = format!("Assigned ID: {}, Port: {}", id, assigned_port);
    let response = Response::new(Body::from(response_body));

    Ok(response)
}

fn spawn_bot_server(bot_data: BotData, assigned_port: u16, bots: Arc<Bots>) {
    tokio::spawn(start_bot_server(bot_data, assigned_port));
}

async fn start_bot_server(bot_data: BotData, assigned_port: u16) {
    let addr = SocketAddr::from(([127, 0, 0, 1], assigned_port));

    let make_service = make_service_fn(move |_| {
        let bot_data = bot_data.clone();
        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                forward_request_to_bot(bot_data.clone(), req)
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_service);

    if let Err(e) = server.await {
        eprintln!("Bot server error: {}", e);
    }
}

async fn forward_request_to_bot(
    bot_data: BotData,
    req: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
    info!("{:?}", req);
    let client = Client::new();

    let uri_str = format!("http://127.0.0.1:{}", bot_data.bot_port);
    info!(
        "bot ip: {:?}, bot port sending info to: {:?}",
        bot_data.ip, bot_data.bot_port
    );
    let uri = Uri::from_str(&uri_str).unwrap();

    // Clone the request and modify the URI.
    let (parts, body) = req.into_parts();
    let mut new_req = Request::from_parts(parts, body);
    *new_req.uri_mut() = uri;

    let response = client.request(new_req).await.unwrap();
    Ok(response)
}

async fn cleanup_bots(bots: Arc<Bots>) {
    let mut interval = interval(Duration::from_secs(10));
    loop {
        interval.tick().await;
        let mut bots_lock = bots.bots.lock().unwrap();
        bots_lock.retain(|_, v| {
            let elapsed = v
                .last_communication
                .elapsed()
                .unwrap_or(Duration::new(0, 0));
            elapsed < Duration::from_secs(30)
        });

        for (id, bot) in bots_lock.iter() {
            info!(
                "Bot ID: {}, IP: {}, Port: {}",
                id, bot.bot_data.ip, bot.assigned_port
            );
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::init();
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let bots = Arc::new(Bots::new());

    let bots_for_cleanup = bots.clone();
    tokio::spawn(async move {
        cleanup_bots(bots_for_cleanup).await;
    });

    let make_service = make_service_fn(move |_| {
        let bots = bots.clone();
        async {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                handle_registration(bots.clone(), req)
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_service);
    info!("Server is running on {}", addr);

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }

    Ok(())
}
