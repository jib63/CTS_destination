/// Quick diagnostic binary for the Pixoo64 HTTP API.
///
/// Usage:  cargo run --bin pixoo_test -- <host:port>
///   e.g.  cargo run --bin pixoo_test -- 192.168.1.189:80
///
/// Tries several brightness command variants and prints the raw response for
/// each, so you can see exactly which format the device firmware accepts.

use std::time::Duration;
use reqwest::Client;

#[tokio::main]
async fn main() {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "192.168.1.189:80".to_string());

    let url = format!("http://{}/post", addr);
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    println!("=== Pixoo64 brightness diagnostic — {addr} ===\n");

    section("1. Channel/SetBrightness — 50% (correct command)");
    post(&client, &url, serde_json::json!({
        "Command":    "Channel/SetBrightness",
        "Brightness": 50,
    })).await;

    section("2. Channel/SetBrightness — 100% (restore)");
    post(&client, &url, serde_json::json!({
        "Command":    "Channel/SetBrightness",
        "Brightness": 100,
    })).await;

    section("3. Device/SetHighLightMode — Mode 1 (maximum brightness override)");
    post(&client, &url, serde_json::json!({
        "Command": "Device/SetHighLightMode",
        "Mode":    1,
    })).await;

    tokio::time::sleep(Duration::from_secs(2)).await;

    section("4. Device/SetHighLightMode — Mode 0 (normal)");
    post(&client, &url, serde_json::json!({
        "Command": "Device/SetHighLightMode",
        "Mode":    0,
    })).await;

    section("5. Channel/GetIndex — which channel is active?");
    post(&client, &url, serde_json::json!({
        "Command": "Channel/GetIndex",
    })).await;

    section("6. Device/GetDeviceTime — connectivity check");
    post(&client, &url, serde_json::json!({
        "Command": "Device/GetDeviceTime",
    })).await;
}

fn section(title: &str) {
    println!("── {title}");
}

async fn post(client: &Client, url: &str, body: serde_json::Value) {
    println!("   body : {body}");
    match client.post(url).json(&body).send().await {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_else(|_| "(unreadable)".into());
            println!("   resp : {status}  {text}\n");
        }
        Err(e) => println!("   ERR  : {e}\n"),
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
}
