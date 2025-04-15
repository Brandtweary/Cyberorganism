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
use std::io::Error as IoError;
use std::net::TcpListener;
use std::error::Error;
use std::fs;
use std::time::Duration;

// Import our datastore module
mod logseq_datastore;
use logseq_datastore::{LogseqDatastore, LogseqBlockData, LogseqPageData};

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
    // #[serde(rename = "graphName")]
    // graph_name: String,
    #[serde(default)]
    type_: Option<String>,
    payload: String,
}

// Constants
const PID_FILE: &str = "logseq_knowledge_graph_server.pid";
const DEFAULT_PORT: u16 = 3000;

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
    "Logseq Knowledge Graph Backend Server"
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
fn parse_block_data(payload: &str) -> Result<LogseqBlockData, serde_json::Error> {
    serde_json::from_str::<LogseqBlockData>(payload)
}

fn parse_page_data(payload: &str) -> Result<LogseqPageData, serde_json::Error> {
    serde_json::from_str::<LogseqPageData>(payload)
}

// Helper function for handling block data
fn handle_block_data(state: Arc<AppState>, payload: &str) -> Result<String, String> {
    // Parse the payload as a LogseqBlockData
    let block_data = parse_block_data(payload)
        .map_err(|e| format!("Could not parse block data: {e}"))?;
    
    // Validate the block data
    if block_data.id.is_empty() {
        return Err("Block ID is empty".to_string());
    }
    
    // Process the block data
    let mut datastore = state.datastore.lock().unwrap();
    
    match datastore.create_or_update_node_from_logseq_block(&block_data) {
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
    // Parse the payload as a LogseqPageData
    let page_data = parse_page_data(payload)
        .map_err(|e| format!("Could not parse page data: {e}"))?;
    
    // Validate the page data
    if page_data.name.is_empty() {
        return Err("Page name is empty".to_string());
    }
    
    // Process the page data
    let mut datastore = state.datastore.lock().unwrap();
    
    match datastore.create_or_update_node_from_logseq_page(&page_data) {
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

// Helper function for analyzing references
fn analyze_references(references: &[serde_json::Value]) -> (i32, i32, i32, i32) {
    let mut page_refs = 0;
    let mut block_refs = 0;
    let mut tags = 0;
    let mut properties = 0;
    
    for ref_value in references {
        if let Some(ref_obj) = ref_value.as_object() {
            if let Some(ref_type) = ref_obj.get("type").and_then(|t| t.as_str()) {
                match ref_type {
                    "page" => {
                        page_refs += 1;
                    },
                    "block" => {
                        block_refs += 1;
                    },
                    "tag" => {
                        tags += 1;
                    },
                    "property" => {
                        properties += 1;
                    },
                    _ => {}
                }
            }
        }
    }
    
    (page_refs, block_refs, tags, properties)
}

// Helper function for printing reference summary
fn print_reference_summary(page_refs: i32, block_refs: i32, tags: i32, properties: i32) {
    println!("Reference summary:");
    println!("  - Page refs: {page_refs}");
    println!("  - Block refs: {block_refs}");
    println!("  - Tags: {tags}");
    println!("  - Properties: {properties}");
}

// Helper function for handling test references
fn handle_test_references(payload: &str) -> Result<String, String> {
    // Parse the payload as JSON
    let json_value: serde_json::Value = serde_json::from_str(payload)
        .map_err(|e| format!("Could not parse payload as JSON: {e}"))?;
    
    // Check if the JSON has a "references" field
    if let Some(references) = json_value.get("references") {
        if let Some(refs_array) = references.as_array() {
            if refs_array.is_empty() {
                println!("No references found in block");
            } else {
                let (page_refs, block_refs, tags, properties) = analyze_references(refs_array);
                print_reference_summary(page_refs, block_refs, tags, properties);
                
                // Print additional debug info for the first reference
                if !refs_array.is_empty() {
                    println!("\nFirst reference details:");
                    println!("  {}", serde_json::to_string_pretty(&refs_array[0]).unwrap_or_default());
                    
                    // Check for children
                    if let Some(children) = json_value.get("children") {
                        if let Some(arr) = children.as_array() {
                            if !arr.is_empty() {
                                println!("First child: {}", serde_json::to_string_pretty(&arr[0]).unwrap_or_default());
                                println!("First child type: {}", 
                                    if arr[0].is_string() { "string" }
                                    else if arr[0].is_number() { "number" }
                                    else if arr[0].is_object() { "object" }
                                    else { "unknown" });
                            }
                        }
                    }
                }
            }
        } else {
            println!("References field is not an array");
        }
    } else {
        println!("No references found in block");
    }
    
    println!("=== END TEST REFERENCES ===\n");
    Ok("Test references processed".to_string())
}

// Helper function for handling default data
fn handle_default_data(source: &str) -> Result<String, String> {
    // For DB changes, just acknowledge receipt without verbose logging
    if source == "Logseq DB Change" {
        // Minimal logging for DB changes
        println!("Processing DB change event");
    } else {
        println!("Processing data with unspecified type");
    }
    
    Ok("Data received".to_string())
}

// Helper function for handling batch block data
fn handle_batch_blocks(state: Arc<AppState>, payload: &str) -> Result<String, String> {
    // Parse the payload as an array of LogseqBlockData
    let blocks: Vec<LogseqBlockData> = serde_json::from_str(payload)
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
            match datastore.create_or_update_node_from_logseq_block(&block_data) {
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
    // Parse the payload as an array of LogseqPageData
    let pages: Vec<LogseqPageData> = serde_json::from_str(payload)
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
            match datastore.create_or_update_node_from_logseq_page(&page_data) {
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

// Endpoint to receive data from the Logseq plugin
async fn receive_data(
    State(state): State<Arc<AppState>>,
    Json(data): Json<LogseqData>,
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
        Some("test_references") => {
            match handle_test_references(&data.payload) {
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

    // Try to use the default port
    let mut port = DEFAULT_PORT;
    
    // If default port is not available, find another one
    if !is_port_available(port) {
        println!("Default port {port} is not available.");
        
        // Try a few alternative ports
        for p in (DEFAULT_PORT + 1)..=(DEFAULT_PORT + 10) {
            if is_port_available(p) {
                port = p;
                println!("Using alternative port: {port}");
                break;
            }
        }
        
        if port == DEFAULT_PORT {
            return Err(Box::<dyn Error>::from("Could not find an available port"));
        }
    }
    
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
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
