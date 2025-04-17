/**
 * @module main
 * @description Backend server for the PKM Knowledge Graph Plugin
 * 
 * This module implements an HTTP server that receives data from the PKM frontend plugin,
 * processes it, and stores it in a persistent knowledge graph datastore. It serves as the
 * backend component of the PKM Knowledge Graph extension.
 * 
 * Key responsibilities:
 * - Setting up and running the HTTP server using the Axum framework
 * - Defining API endpoints for data reception and sync status management
 * - Processing incoming PKM data (blocks, pages, and their references)
 * - Managing the PKMDatastore for persistent storage
 * - Handling server lifecycle (startup, shutdown, port management)
 * - Configuration loading and management
 * - Process management (PID tracking, termination of previous instances)
 * 
 * API endpoints:
 * - GET  /          : Root endpoint that confirms the server is running
 * - POST /data      : Main endpoint for receiving data from the PKM plugin
 * - GET  /sync/status: Returns the current sync status (timestamp, node count, etc.)
 * - POST /sync/update: Updates the sync timestamp after a full sync
 * 
 * The server handles several types of data:
 * - Individual blocks and pages
 * - Batches of blocks and pages for efficient processing
 * - Diagnostic information
 * 
 * Dependencies:
 * - pkm_datastore: Module for persistent storage of the knowledge graph
 * - axum: Web framework for handling HTTP requests
 * - tokio: Async runtime
 * - serde: Serialization/deserialization of JSON data
 * 
 * @requires pkm_datastore
 * @requires axum
 * @requires tokio
 * @requires serde
 */

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::process::{Command, exit};
use std::io::Error as IoError;
use std::net::TcpListener;
use std::error::Error;
use std::fs;
use std::time::Duration;

// Import our datastore module
mod pkm_datastore;
use pkm_datastore::{PKMDatastore, PKMBlockData, PKMPageData};

// Application state that will be shared between handlers
struct AppState {
    datastore: Mutex<PKMDatastore>,
}

// Basic response for API calls
#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

// Incoming data from the PKM plugin
#[derive(Deserialize, Debug)]
struct PKMData {
    source: String,
    timestamp: String,
    // #[serde(rename = "graphName")]
    // graph_name: String,
    #[serde(default)]
    type_: Option<String>,
    payload: String,
}

// Configuration structure
#[derive(Debug, Deserialize)]
struct Config {
    backend: BackendConfig,
}

#[derive(Debug, Deserialize)]
struct BackendConfig {
    host: String,
    port: u16,
    max_port_attempts: u16,
}

// Default configuration
impl Default for Config {
    fn default() -> Self {
        Config {
            backend: BackendConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
                max_port_attempts: 10,
            },
        }
    }
}

// Load configuration from file
fn load_config() -> Config {
    // Determine the executable directory
    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = exe_path.parent().unwrap_or_else(|| Path::new("."));
    
    // Try to find config.yaml in parent directories
    let mut config_path = PathBuf::from(exe_dir);
    let mut found = false;
    
    // First check if config exists in the current directory
    if config_path.join("config.yaml").exists() {
        found = true;
    } else {
        // Try up to 3 parent directories
        for _ in 0..3 {
            config_path = match config_path.parent() {
                Some(parent) => parent.to_path_buf(),
                None => break,
            };
            
            if config_path.join("config.yaml").exists() {
                found = true;
                break;
            }
        }
    }
    
    // If config.yaml was found, try to load it
    if found {
        let config_file = config_path.join("config.yaml");
        match fs::read_to_string(&config_file) {
            Ok(contents) => {
                match serde_yaml::from_str(&contents) {
                    Ok(config) => {
                        println!("Loaded configuration from {:?}", config_file);
                        return config;
                    },
                    Err(e) => {
                        println!("Error parsing config.yaml: {}", e);
                    }
                }
            },
            Err(e) => {
                println!("Error reading config.yaml: {}", e);
            }
        }
    }
    
    // If we get here, use default configuration
    println!("Using default configuration");
    Config::default()
}

// Constants
const PID_FILE: &str = "pkm_knowledge_graph_server.pid";

// Check if a port is available
fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

// Try to terminate a previous instance of our server
fn terminate_previous_instance() -> bool {
    // Check if PID file exists
    if let Ok(pid_str) = fs::read_to_string(PID_FILE) {
        let pid = pid_str.trim();
        
        println!("Found previous instance with PID: {pid}");
        
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
                    }
                    println!("Failed to terminate process: {}", 
                        String::from_utf8_lossy(&output.stderr));
                },
                Err(e) => {
                    println!("Error terminating process: {e}");
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
    fs::write(PID_FILE, pid)?;
    Ok(())
}

// Clean up PID file on exit
fn setup_exit_handler() {
    ctrlc::set_handler(move || {
        println!("Received shutdown signal, cleaning up...");
        if let Err(e) = fs::remove_file(PID_FILE) {
            println!("Error removing PID file: {e}");
        }
        exit(0);
    }).expect("Error setting Ctrl-C handler");
}

// Root endpoint
async fn root() -> &'static str {
    "PKM Knowledge Graph Backend Server"
}

// Endpoint to get sync status
async fn get_sync_status(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let status = state.datastore.lock().unwrap().get_sync_status();
    Json(status)
}

// Endpoint to update sync timestamp after a full sync
async fn update_sync_timestamp(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse> {
    let mut datastore = state.datastore.lock().unwrap();
    
    match datastore.update_full_sync_timestamp() {
        Ok(()) => {
            println!("Sync timestamp updated successfully");
            Json(ApiResponse {
                success: true,
                message: "Sync timestamp updated successfully".to_string(),
            })
        },
        Err(e) => {
            println!("Error updating sync timestamp: {e:?}");
            Json(ApiResponse {
                success: false,
                message: format!("Error updating sync timestamp: {e:?}"),
            })
        }
    }
}

// Helper functions for data parsing
fn parse_block_data(payload: &str) -> Result<PKMBlockData, serde_json::Error> {
    serde_json::from_str::<PKMBlockData>(payload)
}

fn parse_page_data(payload: &str) -> Result<PKMPageData, serde_json::Error> {
    serde_json::from_str::<PKMPageData>(payload)
}

// Helper function for handling block data
fn handle_block_data(state: Arc<AppState>, payload: &str) -> Result<String, String> {
    // Parse the payload as a PKMBlockData
    let block_data = parse_block_data(payload)
        .map_err(|e| format!("Could not parse block data: {e}"))?;
    
    // Validate the block data
    if block_data.id.is_empty() {
        return Err("Block ID is empty".to_string());
    }
    
    // Process the block data
    let mut datastore = state.datastore.lock().unwrap();
    
    match datastore.create_or_update_node_from_pkm_block(&block_data) {
        Ok(node_id) => {
            println!("Block processed successfully: {node_id}");
            let result = datastore.save_state()
                .map_err(|e| format!("Error saving state: {e:?}"));
            drop(datastore);
            result?;
            Ok("Block processed successfully".to_string())
        },
        Err(e) => {
            drop(datastore);
            Err(format!("Error processing block: {e:?}"))
        }
    }
}

// Helper function for handling page data
fn handle_page_data(state: Arc<AppState>, payload: &str) -> Result<String, String> {
    // Parse the payload as a PKMPageData
    let page_data = parse_page_data(payload)
        .map_err(|e| format!("Could not parse page data: {e}"))?;
    
    // Validate the page data
    if page_data.name.is_empty() {
        return Err("Page name is empty".to_string());
    }
    
    // Process the page data
    let mut datastore = state.datastore.lock().unwrap();
    
    match datastore.create_or_update_node_from_pkm_page(&page_data) {
        Ok(node_id) => {
            println!("Page processed successfully: {node_id}");
            let result = datastore.save_state()
                .map_err(|e| format!("Error saving state: {e:?}"));
            drop(datastore);
            result?;
            Ok("Page processed successfully".to_string())
        },
        Err(e) => {
            drop(datastore);
            Err(format!("Error processing page: {e:?}"))
        }
    }
}

// Helper function for handling default data
fn handle_default_data(source: &str) -> Result<String, String> {
    // For DB changes, just acknowledge receipt without verbose logging
    if source == "PKM DB Change" {
        // Minimal logging for DB changes
        println!("Processing DB change event");
    } else {
        println!("Processing data with unspecified type");
    }
    
    Ok("Data received".to_string())
}

// Helper function for handling batch block data
fn handle_batch_blocks(state: Arc<AppState>, payload: &str) -> Result<String, String> {
    // Parse the payload as an array of PKMBlockData
    let blocks: Vec<PKMBlockData> = serde_json::from_str(payload)
        .map_err(|e| format!("Could not parse batch blocks: {e}"))?;
    
    println!("Processing batch of {} blocks", blocks.len());
    
    let mut success_count = 0;
    let mut error_count = 0;
    let total_blocks = blocks.len();
    
    // Get a single lock on the datastore for the entire batch
    let mut datastore = state.datastore.lock().unwrap();
    
    for block_data in blocks {
        // Validate and process each block
        if block_data.validate().is_ok() {
            match datastore.create_or_update_node_from_pkm_block(&block_data) {
                Ok(_) => {
                    success_count += 1;
                },
                Err(_) => {
                    error_count += 1;
                }
            }
        } else {
            error_count += 1;
        }
    }
    
    // Save state once after processing the entire batch
    if success_count > 0 {
        if let Err(e) = datastore.save_state() {
            println!("Error saving state after batch processing: {e:?}");
        }
    }
    
    // Release the lock
    drop(datastore);
    
    // Report results
    if error_count == 0 {
        Ok(format!("Successfully processed all {total_blocks} blocks"))
    } else if success_count > 0 {
        Ok(format!("Processed {success_count}/{total_blocks} blocks successfully, {error_count} errors"))
    } else {
        Err(format!("Failed to process any blocks, {error_count} errors"))
    }
}

// Helper function for handling batch page data
fn handle_batch_pages(state: Arc<AppState>, payload: &str) -> Result<String, String> {
    // Parse the payload as an array of PKMPageData
    let pages: Vec<PKMPageData> = serde_json::from_str(payload)
        .map_err(|e| format!("Could not parse batch pages: {e}"))?;
    
    println!("Processing batch of {} pages", pages.len());
    
    let mut success_count = 0;
    let mut error_count = 0;
    let total_pages = pages.len();
    
    // Get a single lock on the datastore for the entire batch
    let mut datastore = state.datastore.lock().unwrap();
    
    for page_data in pages {
        // Validate and process each page
        if page_data.validate().is_ok() {
            match datastore.create_or_update_node_from_pkm_page(&page_data) {
                Ok(_) => {
                    success_count += 1;
                },
                Err(_) => {
                    error_count += 1;
                }
            }
        } else {
            error_count += 1;
        }
    }
    
    // Save state once after processing the entire batch
    if success_count > 0 {
        if let Err(e) = datastore.save_state() {
            println!("Error saving state after batch processing: {e:?}");
        }
    }
    
    // Release the lock
    drop(datastore);
    
    // Report results
    if error_count == 0 {
        Ok(format!("Successfully processed all {total_pages} pages"))
    } else if success_count > 0 {
        Ok(format!("Processed {success_count}/{total_pages} pages successfully, {error_count} errors"))
    } else {
        Err(format!("Failed to process any pages, {error_count} errors"))
    }
}

// Endpoint to receive data from the PKM plugin
async fn receive_data(
    State(state): State<Arc<AppState>>,
    Json(data): Json<PKMData>,
) -> Json<ApiResponse> {
    // Log the source of the data
    println!("Received data from: {} at {}", data.source, data.timestamp);
    
    // Process based on the type of data
    match data.type_.as_deref() {
        Some("block") => {
            match handle_block_data(state, &data.payload) {
                Ok(message) => {
                    Json(ApiResponse {
                        success: true,
                        message,
                    })
                },
                Err(message) => {
                    Json(ApiResponse {
                        success: false,
                        message,
                    })
                }
            }
        },
        Some("block_batch") | Some("blocks") => {
            match handle_batch_blocks(state, &data.payload) {
                Ok(message) => {
                    Json(ApiResponse {
                        success: true,
                        message,
                    })
                },
                Err(message) => {
                    Json(ApiResponse {
                        success: false,
                        message,
                    })
                }
            }
        },
        Some("page") => {
            match handle_page_data(state, &data.payload) {
                Ok(message) => {
                    Json(ApiResponse {
                        success: true,
                        message,
                    })
                },
                Err(message) => {
                    Json(ApiResponse {
                        success: false,
                        message,
                    })
                }
            }
        },
        Some("page_batch") | Some("pages") => {
            match handle_batch_pages(state, &data.payload) {
                Ok(message) => {
                    Json(ApiResponse {
                        success: true,
                        message,
                    })
                },
                Err(message) => {
                    Json(ApiResponse {
                        success: false,
                        message,
                    })
                }
            }
        },
        // For DB change events and other unspecified types
        _ => {
            match handle_default_data(&data.source) {
                Ok(message) => {
                    Json(ApiResponse {
                        success: true,
                        message,
                    })
                },
                Err(message) => {
                    Json(ApiResponse {
                        success: false,
                        message,
                    })
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Set up exit handler to clean up PID file
    setup_exit_handler();
    
    // Load configuration
    let config = load_config();
    
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
    let datastore = PKMDatastore::new(data_dir)
        .map_err(|e| Box::<dyn Error>::from(format!("Datastore error: {e:?}")))?;
    
    // Create shared application state
    let app_state = Arc::new(AppState {
        datastore: Mutex::new(datastore),
    });
    
    // Define the application routes
    let app = Router::new()
        .route("/", get(root))
        .route("/data", post(receive_data))
        .route("/sync/status", get(get_sync_status))
        .route("/sync/update", post(update_sync_timestamp))
        .with_state(app_state);

    // Try to use the configured port
    let mut port = config.backend.port;
    
    // If configured port is not available, find another one
    if !is_port_available(port) {
        println!("Configured port {port} is not available.");
        
        // Try a few alternative ports
        for p in (port + 1)..=(port + config.backend.max_port_attempts) {
            if is_port_available(p) {
                port = p;
                println!("Using alternative port: {port}");
                break;
            }
        }
        
        if port == config.backend.port {
            return Err(Box::<dyn Error>::from("Could not find an available port"));
        }
    }
    
    let host_parts: Vec<&str> = config.backend.host.split('.').collect();
    let host_addr = if host_parts.len() == 4 {
        // Parse IP address like "127.0.0.1"
        let parts: Result<Vec<u8>, _> = host_parts.iter().map(|s| s.parse::<u8>()).collect();
        match parts {
            Ok(bytes) if bytes.len() == 4 => [bytes[0], bytes[1], bytes[2], bytes[3]],
            _ => [127, 0, 0, 1], // Fallback to localhost
        }
    } else {
        // Default to localhost if not a valid IP
        [127, 0, 0, 1]
    };
    
    let addr = SocketAddr::from((host_addr, port));
    println!("Backend server listening on {addr}");

    // Run the server
    let listener = tokio::net::TcpListener::bind(addr).await
        .map_err(|e| Box::<dyn Error>::from(format!("Listener error: {e}")))?;
    
    axum::serve(listener, app).await
        .map_err(|e| Box::<dyn Error>::from(format!("Server error: {e}")))?;
    
    // Clean up PID file before exiting
    if let Err(e) = fs::remove_file(PID_FILE) {
        println!("Error removing PID file: {e}");
    }
    
    Ok(())
}
