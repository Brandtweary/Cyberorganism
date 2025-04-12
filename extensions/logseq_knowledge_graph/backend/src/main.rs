use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// Import our datastore module
mod logseq_datastore;
use logseq_datastore::{Block, LogseqDatastore, Page};

// Application state that will be shared between handlers
struct AppState {
    datastore: Mutex<LogseqDatastore>,
}

// Basic response for API calls
#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

// Incoming data from the Logseq plugin
#[derive(Deserialize, Debug)]
struct LogseqData {
    source: String,
    timestamp: String,
    graphName: String,
    payload: String,
    // We'll expand this with actual block/page data later
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the datastore
    let data_dir = PathBuf::from("data");
    let datastore = LogseqDatastore::new(data_dir)?;
    
    // Create shared application state
    let app_state = Arc::new(AppState {
        datastore: Mutex::new(datastore),
    });
    
    // Define the application routes
    let app = Router::new()
        .route("/", get(root))
        .route("/data", post(receive_data))
        .with_state(app_state);

    // Define the address to listen on
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Backend server listening on {}", addr);

    // Run the server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}

// Basic handler for the root path
async fn root() -> &'static str {
    "Logseq Knowledge Graph Backend Server is running!"
}

// Handler for receiving data from Logseq plugin
async fn receive_data(
    State(state): State<Arc<AppState>>, 
    Json(data): Json<LogseqData>
) -> Json<ApiResponse> {
    println!("Received data from plugin:");
    println!("{:#?}", data);
    
    // For now, just acknowledge receipt
    // In the future, we'll process and store the actual block data
    
    Json(ApiResponse {
        success: true,
        message: format!("Received data from graph: {}", data.graphName),
    })
}
