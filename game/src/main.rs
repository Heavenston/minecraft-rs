#![allow(dead_code)]

use anyhow::Result;

mod chunk;
mod resource_location;

struct App {
    
}

impl engine::App for App { }

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    engine::start(App { })
}
