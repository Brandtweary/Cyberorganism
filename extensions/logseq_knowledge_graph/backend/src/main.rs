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
    fs::write(PID_FILE, pid)?;
    Ok(())
}

// Clean up PID file on exit
fn setup_exit_handler() {
    ctrlc::set_handler(move || {
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

// Endpoint to get sync status
async fn get_sync_status(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let datastore = state.datastore.lock().unwrap();
    let status = datastore.get_sync_status();
    
    Json(status)
}

// Endpoint to update sync timestamp after a full sync
async fn update_sync_timestamp(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse> {
    let mut datastore = state.datastore.lock().unwrap();
    
    match datastore.update_full_sync_timestamp() {
        Ok(_) => {
            println!("Sync timestamp updated successfully");
            Json(ApiResponse {
                success: true,
                message: "Sync timestamp updated successfully".to_string(),
            })
        },
        Err(e) => {
            println!("Error updating sync timestamp: {:?}", e);
            Json(ApiResponse {
                success: false,
                message: format!("Error updating sync timestamp: {:?}", e),
            })
        }
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
            // Parse the payload as a LogseqBlockData
            match serde_json::from_str::<LogseqBlockData>(&data.payload) {
                Ok(block_data) => {
                    // Validate the block data
                    match block_data.validate() {
                        Ok(_) => {
                            // Process the block data
                            let mut datastore = state.datastore.lock().unwrap();
                            match datastore.create_or_update_node_from_logseq_block(&block_data) {
                                Ok(node_id) => {
                                    Json(ApiResponse {
                                        success: true,
                                        message: format!("Block processed successfully. Node ID: {}", node_id),
                                    })
                                },
                                Err(e) => {
                                    println!("Error processing block: {:?}", e);
                                    
                                    Json(ApiResponse {
                                        success: false,
                                        message: format!("Error processing block: {:?}", e),
                                    })
                                }
                            }
                        },
                        Err(validation_errors) => {
                            println!("Block data validation failed: {}", validation_errors);
                            Json(ApiResponse {
                                success: false,
                                message: format!("Block data validation failed: {}", validation_errors),
                            })
                        }
                    }
                },
                Err(e) => {
                    // Print more detailed error information
                    println!("Could not parse block data: {}", e);
                    println!("Raw block data (first 200 chars): {}", 
                             if data.payload.len() > 200 { 
                                 &data.payload[..200] 
                             } else { 
                                 &data.payload 
                             });
                    
                    // Try to parse as a generic Value to see what fields are present
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&data.payload) {
                        println!("Block data structure: {}", 
                                 serde_json::to_string_pretty(&value).unwrap_or_else(|_| "Could not pretty-print".to_string()));
                        
                        // Check specific fields that might cause problems
                        if let Some(obj) = value.as_object() {
                            if let Some(parent) = obj.get("parent") {
                                println!("Parent field type: {}", 
                                         if parent.is_null() { "null" } 
                                         else if parent.is_string() { "string" }
                                         else if parent.is_number() { "number" }
                                         else if parent.is_object() { "object" }
                                         else { "unknown" });
                            }
                            
                            if let Some(children) = obj.get("children") {
                                println!("Children field type: {}", 
                                         if children.is_null() { "null" } 
                                         else if children.is_array() { "array" }
                                         else { "unknown" });
                                
                                if children.is_array() {
                                    if let Some(arr) = children.as_array() {
                                        if !arr.is_empty() {
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
                    }
                    
                    Json(ApiResponse {
                        success: false,
                        message: format!("Could not parse block data: {}", e),
                    })
                }
            }
        },
        Some("block_batch") => {
            // Parse the payload as an array of LogseqBlockData
            match serde_json::from_str::<Vec<LogseqBlockData>>(&data.payload) {
                Ok(blocks) => {
                    println!("Processing batch of {} blocks", blocks.len());
                    
                    let mut datastore = state.datastore.lock().unwrap();
                    let mut success_count = 0;
                    let mut error_count = 0;
                    let total_blocks = blocks.len();
                    
                    for block_data in &blocks {
                        // Validate the block data
                        match block_data.validate() {
                            Ok(_) => {
                                // Process the block data
                                match datastore.create_or_update_node_from_logseq_block(block_data) {
                                    Ok(_) => {
                                        success_count += 1;
                                    },
                                    Err(_) => {
                                        error_count += 1;
                                    }
                                }
                            },
                            Err(_) => {
                                error_count += 1;
                            }
                        }
                    }
                    
                    Json(ApiResponse {
                        success: error_count == 0,
                        message: format!("Processed {} blocks: {} successful, {} failed", 
                                        total_blocks, success_count, error_count),
                    })
                },
                Err(e) => {
                    println!("Could not parse block batch: {}", e);
                    
                    Json(ApiResponse {
                        success: false,
                        message: format!("Could not parse block batch: {}", e),
                    })
                }
            }
        },
        Some("page") => {
            // Parse the payload as a LogseqPageData
            match serde_json::from_str::<LogseqPageData>(&data.payload) {
                Ok(page_data) => {
                    // Validate the page data
                    match page_data.validate() {
                        Ok(_) => {
                            // Process the page data
                            let mut datastore = state.datastore.lock().unwrap();
                            match datastore.create_or_update_node_from_logseq_page(&page_data) {
                                Ok(node_id) => {
                                    Json(ApiResponse {
                                        success: true,
                                        message: format!("Page processed successfully. Node ID: {}", node_id),
                                    })
                                },
                                Err(e) => {
                                    println!("Error processing page: {:?}", e);
                                    
                                    Json(ApiResponse {
                                        success: false,
                                        message: format!("Error processing page: {:?}", e),
                                    })
                                }
                            }
                        },
                        Err(validation_errors) => {
                            println!("Page data validation failed: {}", validation_errors);
                            Json(ApiResponse {
                                success: false,
                                message: format!("Page data validation failed: {}", validation_errors),
                            })
                        }
                    }
                },
                Err(e) => {
                    println!("Could not parse page data: {}", e);
                    
                    Json(ApiResponse {
                        success: false,
                        message: format!("Could not parse page data: {}", e),
                    })
                }
            }
        },
        Some("page_batch") => {
            // Parse the payload as an array of LogseqPageData
            match serde_json::from_str::<Vec<LogseqPageData>>(&data.payload) {
                Ok(pages) => {
                    println!("Processing batch of {} pages", pages.len());
                    
                    let mut datastore = state.datastore.lock().unwrap();
                    let mut success_count = 0;
                    let mut error_count = 0;
                    let total_pages = pages.len();
                    
                    for page_data in &pages {
                        // Validate the page data
                        match page_data.validate() {
                            Ok(_) => {
                                // Process the page data
                                match datastore.create_or_update_node_from_logseq_page(page_data) {
                                    Ok(_) => {
                                        success_count += 1;
                                    },
                                    Err(_) => {
                                        error_count += 1;
                                    }
                                }
                            },
                            Err(_) => {
                                error_count += 1;
                            }
                        }
                    }
                    
                    Json(ApiResponse {
                        success: error_count == 0,
                        message: format!("Processed {} pages: {} successful, {} failed", 
                                        total_pages, success_count, error_count),
                    })
                },
                Err(e) => {
                    println!("Could not parse page batch: {}", e);
                    
                    Json(ApiResponse {
                        success: false,
                        message: format!("Could not parse page batch: {}", e),
                    })
                }
            }
        },
        Some("diagnostic") => {
            println!("\n=== DIAGNOSTIC INFO ===");
            
            // Parse the diagnostic payload
            match serde_json::from_str::<serde_json::Value>(&data.payload) {
                Ok(value) => {
                    let message_text = value.get("message").and_then(|m| m.as_str()).unwrap_or("");
                    if !message_text.is_empty() {
                        println!("Message: {}", message_text);
                    }
                    
                    if let Some(details) = value.get("details") {
                        // Format the validation issue details with breakdown by type
                        if message_text.contains("Validation issues summary") {
                            println!("Details: {{");
                            
                            // Process blockIssuesByPage with type breakdown
                            if let Some(block_issues) = details.get("blockIssuesByPage") {
                                if let Some(block_issues_obj) = block_issues.as_object() {
                                    println!("  \"blockIssuesByPage\": {{");
                                    
                                    for (page_name, issues) in block_issues_obj {
                                        if let Some(issue_obj) = issues.as_object() {
                                            if let (Some(total), Some(breakdown)) = (issue_obj.get("total"), issue_obj.get("breakdown")) {
                                                println!("    \"{}\": {} ({}),", page_name, total, breakdown.as_str().unwrap_or(""));
                                            } else {
                                                println!("    \"{}\": {},", page_name, issues);
                                            }
                                        } else {
                                            println!("    \"{}\": {},", page_name, issues);
                                        }
                                    }
                                    
                                    println!("  }},");
                                } else {
                                    println!("  \"blockIssuesByPage\": {},", serde_json::to_string_pretty(block_issues).unwrap_or_else(|_| block_issues.to_string()));
                                }
                            }
                            
                            // Process pageIssues with type breakdown
                            if let Some(page_issues) = details.get("pageIssues") {
                                if let Some(page_issues_obj) = page_issues.as_object() {
                                    println!("  \"pageIssues\": {{");
                                    
                                    for (page_name, issues) in page_issues_obj {
                                        if let Some(issue_obj) = issues.as_object() {
                                            if let (Some(total), Some(breakdown)) = (issue_obj.get("total"), issue_obj.get("breakdown")) {
                                                println!("    \"{}\": {} ({}),", page_name, total, breakdown.as_str().unwrap_or(""));
                                            } else {
                                                println!("    \"{}\": {},", page_name, issues);
                                            }
                                        } else {
                                            println!("    \"{}\": {},", page_name, issues);
                                        }
                                    }
                                    
                                    println!("  }},");
                                } else {
                                    println!("  \"pageIssues\": {},", serde_json::to_string_pretty(page_issues).unwrap_or_else(|_| page_issues.to_string()));
                                }
                            }
                            
                            // Print total counts
                            if let Some(total_block_issues) = details.get("totalBlockIssues") {
                                println!("  \"totalBlockIssues\": {},", total_block_issues);
                            }
                            
                            if let Some(total_page_issues) = details.get("totalPageIssues") {
                                println!("  \"totalPageIssues\": {}", total_page_issues);
                            }
                            
                            println!("}}");
                        } else {
                            // For non-validation diagnostics, just pretty print
                            println!("Details: {}", serde_json::to_string_pretty(details).unwrap_or_else(|_| details.to_string()));
                        }
                    }
                    
                    println!("=== END DIAGNOSTIC INFO ===\n");
                },
                Err(e) => {
                    println!("Could not parse diagnostic data: {}", e);
                    println!("Raw payload: {}", data.payload);
                    println!("=== END DIAGNOSTIC INFO ===\n");
                }
            }
            
            Json(ApiResponse {
                success: true,
                message: "Diagnostic info received".to_string(),
            })
        },
        Some("test_references") => {
            println!("=== TEST REFERENCES ===");
            println!("Received reference test data");
            
            // This is just for testing, we can keep it for now
            match serde_json::from_str::<serde_json::Value>(&data.payload) {
                Ok(value) => {
                    if let Some(references) = value.get("references").and_then(|r| r.as_array()) {
                        println!("Found {} references:", references.len());
                        
                        let mut page_refs = 0;
                        let mut block_refs = 0;
                        let mut tags = 0;
                        let mut properties = 0;
                        
                        for reference in references {
                            if let Some(ref_type) = reference.get("type").and_then(|t| t.as_str()) {
                                println!("  - Type: {}", ref_type);
                                
                                match ref_type {
                                    "page" => {
                                        if let Some(name) = reference.get("name").and_then(|n| n.as_str()) {
                                            println!("    Page: {}", name);
                                        }
                                        page_refs += 1;
                                    },
                                    "block" => {
                                        if let Some(id) = reference.get("id").and_then(|i| i.as_str()) {
                                            println!("    Block ID: {}", id);
                                        }
                                        block_refs += 1;
                                    },
                                    "tag" => {
                                        if let Some(name) = reference.get("name").and_then(|n| n.as_str()) {
                                            println!("    Tag: {}", name);
                                        }
                                        tags += 1;
                                    },
                                    "property" => {
                                        if let Some(name) = reference.get("name").and_then(|n| n.as_str()) {
                                            println!("    Property: {}", name);
                                        }
                                        properties += 1;
                                    },
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
        .route("/sync/status", get(get_sync_status))
        .route("/sync/update", post(update_sync_timestamp))
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
