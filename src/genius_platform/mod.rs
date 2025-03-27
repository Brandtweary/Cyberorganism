//! Genius Platform API integration.
//! 
//! This module provides integration with the Genius Platform API,
//! allowing the application to query the API and display results.
//! Currently not used in the UI but maintained for future integration.

#![allow(dead_code)]

pub mod genius_api;
pub mod genius_api_bridge;
// Removed genius_keyhandler module as it's been moved to archive

// Re-export key types for convenience
// Commenting out unused re-export to fix warning
// pub use genius_api::GeniusItem;
pub use genius_api_bridge::GeniusApiBridge;
// Removing unused factory re-export

use std::sync::Mutex;
use lazy_static::lazy_static;
use std::env;
use dotenv::dotenv;
use tokio::runtime::Runtime;

// Create a global instance of GeniusApiBridge
// This allows us to have a single instance that's shared throughout the application
lazy_static! {
    pub static ref GENIUS_API_BRIDGE: Mutex<GeniusApiBridge> = Mutex::new(GeniusApiBridge::new());
    
    // Create a Tokio runtime for async operations
    pub static ref TOKIO_RUNTIME: Runtime = Runtime::new().expect("Failed to create Tokio runtime");
}

/// Get a reference to the global GeniusApiBridge
/// 
/// This function provides access to the global GeniusApiBridge instance.
/// It's a convenience wrapper around the GENIUS_API_BRIDGE static.
pub fn get_api_bridge() -> std::sync::MutexGuard<'static, GeniusApiBridge> {
    GENIUS_API_BRIDGE.lock().unwrap()
}

/// Initialize the Genius API with credentials from environment variables
///
/// This function attempts to load the API key and organization ID from
/// environment variables and configure the API bridge with them.
///
/// Environment variables:
/// - GENIUS_API_KEY: The API key for authenticating with the Genius API
/// - GENIUS_ORGANIZATION_ID: The organization ID for the Genius API
///
/// Returns true if the API was successfully configured, false otherwise.
pub fn initialize_from_env() -> bool {
    println!("[DEBUG] Initializing Genius API from environment variables");
    
    // Load .env file if it exists
    match dotenv() {
        Ok(_) => println!("[DEBUG] Loaded environment variables from .env file"),
        Err(e) => println!("[DEBUG] Could not load .env file: {e}"),
    }
    
    let api_key = env::var("GENIUS_API_KEY").ok();
    let org_id = env::var("GENIUS_ORGANIZATION_ID").ok();
    
    println!("[DEBUG] GENIUS_API_KEY present: {}", api_key.is_some());
    println!("[DEBUG] GENIUS_ORGANIZATION_ID present: {}", org_id.is_some());
    
    if let (Some(api_key), Some(org_id)) = (api_key, org_id) {
        if api_key.trim().is_empty() || org_id.trim().is_empty() {
            println!("[DEBUG] API key or organization ID is empty");
            return false;
        }
        
        println!("[DEBUG] Configuring API bridge with API key and organization ID");
        println!("[DEBUG] Organization ID: '{org_id}'");
        
        // Configure the bridge asynchronously
        #[allow(clippy::await_holding_lock, clippy::significant_drop_tightening)]
        TOKIO_RUNTIME.block_on(async {
            let bridge = get_api_bridge();
            bridge.configure(&api_key, &org_id).await;
        });
        
        true
    } else {
        println!("[DEBUG] Missing environment variables for Genius API");
        false
    }
}

/// Initialize the Genius API with the provided credentials
///
/// This function configures the API bridge with the given API key and organization ID.
#[allow(clippy::await_holding_lock, clippy::significant_drop_tightening)]
pub fn initialize(api_key: &str, organization_id: &str) {
    // Configure the bridge asynchronously
    TOKIO_RUNTIME.block_on(async {
        let bridge = get_api_bridge();
        bridge.configure(api_key, organization_id).await;
    });
}
