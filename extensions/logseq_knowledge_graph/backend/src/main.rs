use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::process::{Command, exit};
use std::io::{Error as IoError, ErrorKind, Write};
use std::net::TcpListener;
use std::error::Error;
use std::fs;
use std::time::Duration;
use tokio::time;

// Import our datastore module
mod logseq_datastore;
use logseq_datastore::LogseqDatastore;

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
    #[serde(default)]
    type_: Option<String>,
    payload: String,
}

// Block data received from Logseq plugin
#[derive(Deserialize, Debug)]
struct LogseqBlockData {
    id: String,
    content: String,
    created: String,
    updated: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    children: Vec<String>,
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    properties: serde_json::Value,
    #[serde(default)]
    references: Vec<String>,
}

// Page data received from Logseq plugin
#[derive(Deserialize, Debug)]
struct LogseqPageData {
    name: String,
    created: String,
    updated: String,
    #[serde(default)]
    properties: serde_json::Value,
    #[serde(default)]
    blocks: Vec<String>,
}

// Constants
const PID_FILE: &str = "logseq_knowledge_graph_server.pid";
const DEFAULT_PORT: u16 = 3000;

// Check if a port is available
fn is_port_available(port: u16) -> bool {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => true,  // Port is available
        Err(_) => false, // Port is in use
    }
}

// Try to terminate a previous instance of our server
fn terminate_previous_instance() -> bool {
    // Check if PID file exists
    if let Ok(pid_str) = fs::read_to_string(PID_FILE) {
        let pid = pid_str.trim();
        
        println!("Found previous instance with PID: {}", pid);
        
        // Try to terminate the process
        #[cfg(target_family = "unix")]
        {
            let kill_result = Command::new("kill")
                .arg("-15") // SIGTERM for graceful shutdown
                .arg(pid)
                .output();
                
            match kill_result {
                Ok(output) => {
                    if output.status.success() {
                        println!("Successfully terminated previous instance");
                        // Give the process time to shut down
                        std::thread::sleep(Duration::from_millis(500));
                        return true;
                    } else {
                        println!("Failed to terminate process: {}", 
                            String::from_utf8_lossy(&output.stderr));
                    }
                },
                Err(e) => {
                    println!("Error terminating process: {}", e);
                }
            }
        }
        
        #[cfg(target_family = "windows")]
        {
            let kill_result = Command::new("taskkill")
                .args(&["/PID", pid, "/F"])
                .output();
                
            match kill_result {
                Ok(output) => {
                    if output.status.success() {
                        println!("Successfully terminated previous instance");
                        // Give the process time to shut down
                        std::thread::sleep(Duration::from_millis(500));
                        return true;
                    } else {
                        println!("Failed to terminate process: {}", 
                            String::from_utf8_lossy(&output.stderr));
                    }
                },
                Err(e) => {
                    println!("Error terminating process: {}", e);
                }
            }
        }
    }
    
    false
}

// Write current PID to file
fn write_pid_file() -> Result<(), IoError> {
    let pid = std::process::id().to_string();
    println!("Writing PID {} to {}", pid, PID_FILE);
    fs::write(PID_FILE, pid)
}

// Clean up PID file on exit
fn setup_exit_handler() {
    ctrlc::set_handler(|| {
        println!("Received shutdown signal, cleaning up...");
        if let Err(e) = fs::remove_file(PID_FILE) {
            println!("Error removing PID file: {}", e);
        }
        exit(0);
    }).expect("Error setting Ctrl-C handler");
}

// Root endpoint
async fn root() -> &'static str {
    "Logseq Knowledge Graph Backend Server"
}

// Endpoint to receive data from the Logseq plugin
async fn receive_data(
    State(state): State<Arc<AppState>>,
    Json(data): Json<LogseqData>,
) -> Json<ApiResponse> {
    // Basic logging for all requests
    println!("Received data from: {}", data.source);
    
    // Process the data based on its type
    match data.type_.as_deref() {
        Some("block") => {
            match serde_json::from_str::<LogseqBlockData>(&data.payload) {
                Ok(block_data) => {
                    println!("Processing block: {}", block_data.id);
                    
                    // Store the block data in our datastore
                    let _datastore = state.datastore.lock().unwrap();
                    // Implementation details would go here
                    
                    Json(ApiResponse {
                        success: true,
                        message: format!("Successfully processed block data for: {}", block_data.id),
                    })
                },
                Err(e) => {
                    println!("Error parsing block data: {}", e);
                    Json(ApiResponse {
                        success: false,
                        message: format!("Error parsing block data: {}", e),
                    })
                }
            }
        },
        Some("page") => {
            match serde_json::from_str::<LogseqPageData>(&data.payload) {
                Ok(page_data) => {
                    println!("Processing page: {}", page_data.name);
                    
                    // Store the page data in our datastore
                    let _datastore = state.datastore.lock().unwrap();
                    // Implementation details would go here
                    
                    Json(ApiResponse {
                        success: true,
                        message: format!("Successfully processed page data for: {}", page_data.name),
                    })
                },
                Err(e) => {
                    println!("Error parsing page data: {}", e);
                    Json(ApiResponse {
                        success: false,
                        message: format!("Error parsing page data: {}", e),
                    })
                }
            }
        },
        // Special case for the Test References slash command
        Some("test_references") => {
            println!("\n=== TEST REFERENCES COMMAND ===");
            
            // Try to parse as a generic JSON to see if it contains references
            match serde_json::from_str::<serde_json::Value>(&data.payload) {
                Ok(json_data) => {
                    if let Some(references) = json_data.get("references").and_then(|r| r.as_array()) {
                        println!("References found in block:");
                        for (i, reference) in references.iter().enumerate() {
                            println!("  {}. {}", i+1, reference);
                        }
                        
                        // Count reference types
                        let mut page_refs = 0;
                        let mut block_refs = 0;
                        let mut tags = 0;
                        let mut properties = 0;
                        
                        for ref_obj in references {
                            if let Some(ref_type) = ref_obj.get("type").and_then(|t| t.as_str()) {
                                match ref_type {
                                    "page" => page_refs += 1,
                                    "block" => block_refs += 1,
                                    "tag" => tags += 1,
                                    "property" => properties += 1,
                                    _ => {}
                                }
                            }
                        }
                        
                        println!("Reference summary:");
                        println!("  - Page refs: {}", page_refs);
                        println!("  - Block refs: {}", block_refs);
                        println!("  - Tags: {}", tags);
                        println!("  - Properties: {}", properties);
                        println!("=== END TEST REFERENCES ===\n");
                    } else {
                        println!("No references found in block");
                        println!("=== END TEST REFERENCES ===\n");
                    }
                },
                Err(e) => {
                    println!("Could not parse payload as JSON: {}", e);
                    println!("=== END TEST REFERENCES ===\n");
                }
            }
            
            Json(ApiResponse {
                success: true,
                message: "Test references processed".to_string(),
            })
        },
        // For DB change events and other unspecified types
        _ => {
            // For DB changes, just acknowledge receipt without verbose logging
            if data.source == "Logseq DB Change" {
                // Minimal logging for DB changes
                println!("Processing DB change event");
            } else {
                println!("Processing data with unspecified type");
            }
            
            Json(ApiResponse {
                success: true,
                message: "Data received".to_string(),
            })
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Set up exit handler to clean up PID file
    setup_exit_handler();
    
    // Check for previous instance and terminate it
    if fs::metadata(PID_FILE).is_ok() {
        terminate_previous_instance();
        // Remove the PID file in case the process doesn't exist anymore
        let _ = fs::remove_file(PID_FILE);
    }
    
    // Write current PID to file
    write_pid_file()?;
    
    // Initialize the datastore
    let data_dir = PathBuf::from("data");
    let datastore = LogseqDatastore::new(data_dir)
        .map_err(|e| Box::<dyn Error>::from(format!("Datastore error: {:?}", e)))?;
    
    // Create shared application state
    let app_state = Arc::new(AppState {
        datastore: Mutex::new(datastore),
    });
    
    // Define the application routes
    let app = Router::new()
        .route("/", get(root))
        .route("/data", post(receive_data))
        .with_state(app_state);

    // Try to use the default port
    let mut port = DEFAULT_PORT;
    
    // If default port is not available, find another one
    if !is_port_available(port) {
        println!("Default port {} is not available.", port);
        
        // Try a few alternative ports
        for p in (DEFAULT_PORT + 1)..=(DEFAULT_PORT + 10) {
            if is_port_available(p) {
                port = p;
                println!("Using alternative port: {}", port);
                break;
            }
        }
        
        if port == DEFAULT_PORT {
            return Err(Box::<dyn Error>::from("Could not find an available port"));
        }
    }
    
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("Backend server listening on {}", addr);

    // Run the server
    let listener = tokio::net::TcpListener::bind(addr).await
        .map_err(|e| Box::<dyn Error>::from(format!("Listener error: {}", e)))?;
    
    axum::serve(listener, app).await
        .map_err(|e| Box::<dyn Error>::from(format!("Server error: {}", e)))?;
    
    // Clean up PID file before exiting
    if let Err(e) = fs::remove_file(PID_FILE) {
        println!("Error removing PID file: {}", e);
    }
    
    Ok(())
}
