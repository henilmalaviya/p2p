pub mod server;

#[tokio::main]
async fn main() {
    if let Err(e) = server::start_server().await {
        eprintln!("Server error: {e}");
    }
}
