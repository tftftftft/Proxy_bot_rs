use hyper::{
    http::Error,
    service::{make_service_fn, service_fn},
    Body, Client, Request, Response, Server, StatusCode, Uri,
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, net::SocketAddr, str::FromStr};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
struct BotData {
    ip: String,
    // Add other fields representing your bot data here
    // For example:
    // name: String,
    version: String,
    bot_port: u16,
    uuid: String, // other_info: String,
}

async fn handle(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let client = Client::new();
    info!("Received request: {:?}", req);
    let host_header_value = req.headers().get("host").unwrap(); // Get the header value
    let host_str = std::str::from_utf8(host_header_value.as_ref()).unwrap(); // Convert it to a str
                                                                             // You now have a str and can use it in your format string
    let new_uri = format!("http://{}{}", host_str, req.uri()); // You now have a str and can use it in your format string

    info!("Mine Uri {}", new_uri);

    let new_uri_working: hyper::Uri = "http://example.com".parse().unwrap();
    info!("OLD Uri {}", new_uri_working);

    // Clone the headers from the original request
    let headers = req.headers().clone();

    // Begin building the new request to be forwarded
    let mut forward_req = Request::builder()
        .method(req.method().clone())
        .uri(new_uri)
        .version(req.version())
        .body(req.into_body())
        .unwrap();

    // Append the headers to the new request
    *forward_req.headers_mut() = headers;
    info!("Request: {:?}", forward_req);
    let resp = client.request(forward_req).await;
    info!("Response: {:?}", resp);
    resp
}
async fn get_ip() -> Result<String, Error> {
    let target_url: Uri = Uri::from_str("http://myexternalip.com/raw").unwrap();
    let client = Client::new();
    let authority = target_url
        .authority()
        .expect("URI has no authority")
        .clone();
    let req = Request::builder()
        .uri(target_url)
        .header("User-Agent", "My-User-Agent")
        .header(hyper::header::HOST, authority.as_str())
        .body(Body::empty())?;

    let response = client.request(req).await.unwrap();
    let body_bytes = hyper::body::to_bytes(response).await.unwrap();
    let body_string = String::from_utf8_lossy(&body_bytes).to_string();

    Ok(body_string.trim().to_string())
}
async fn send_data_to_master(
    master_addr: SocketAddr,
    uuid: String,
) -> Result<StatusCode, hyper::Error> {
    let target_url: Uri = Uri::from_str(&format!("http://{}", master_addr)).unwrap();
    let client = Client::new();

    // Get the external IP
    let external_ip = get_ip().await.unwrap();
    // Create a BotData instance with the external IP
    let bot_data = BotData {
        ip: external_ip,
        version: "1.0".to_string(),
        bot_port: 3000,
        uuid,
    };

    // Serialize the BotData to JSON
    let json_data = serde_json::to_string(&bot_data).unwrap();

    let req = Request::builder()
        .method("POST")
        .uri(target_url)
        .header("User-Agent", "My-User-Agent")
        .header("Content-Type", "application/json")
        .body(Body::from(json_data))
        .unwrap();

    let response = client.request(req).await?;

    Ok(response.status())
}

async fn send_heartbeat_to_master(master_addr: SocketAddr, uuid: String) {
    info!("Starting to send heartbeats...");
    loop {
        let uuid = uuid.clone();
        info!("Starting a new heartbeat iteration...");
        match send_data_to_master(master_addr, uuid).await {
            Ok(status) => info!("Heartbeat sent to master. Status: {}", status),
            Err(e) => error!("Error while sending heartbeat: {}", e),
        }
        info!("Going to sleep before next heartbeat...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

async fn create_uuid() -> String {
    let uuid = Uuid::new_v4();
    info!("uuid: {:?}", uuid);
    uuid.to_string()
}

#[tokio::main]
async fn main() {
    // Set up logging.
    env_logger::builder()
        .format_timestamp(None)
        .format_module_path(false)
        .init();

    info!("Logger is initialized");

    let addr: SocketAddr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let uuid = create_uuid().await;
    let make_service = make_service_fn(|_| async { Ok::<_, Infallible>(service_fn(handle)) });
    let server = Server::bind(&addr).serve(make_service);
    info!("Server is running on {}", addr);

    // Spawn a separate task to send heartbeats to the master server.
    let master_addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let heartbeat_task = tokio::spawn(send_heartbeat_to_master(master_addr, uuid));

    let server_task = tokio::spawn(async move {
        if let Err(e) = server.await {
            error!("Server crashed: {}", e);
        }
    });

    tokio::select! {
        res = server_task => {
            if let Err(e) = res {
                error!("Server crashed: {}", e);
            }
        },
        res = heartbeat_task => {
            if let Err(e) = res {
                error!("Heartbeat task failed: {}", e);
            }
        },
    }
}
