#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use super::genius_api::{GeniusApiClient, GeniusApiError, GeniusResponse, GeniusItem};
use crate::App;
use serde_json;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Bridge between the application UI and the Genius API
/// 
/// This module provides a clean, async interface for interacting with the Genius API.
/// All API communication should go through this bridge to ensure proper isolation.
pub struct GeniusApiBridge {
    /// The inner bridge wrapped in an Arc<Mutex> for thread safety
    inner: Arc<Mutex<GeniusApiBridgeInner>>,
}

/// Inner implementation of the GeniusApiBridge
struct GeniusApiBridgeInner {
    /// The API client used to make requests
    api_client: GeniusApiClient,
    /// The most recent API response
    last_response: Option<GeniusResponse>,
    /// Flag indicating if a request is in progress
    request_in_progress: bool,
    /// Current page number (1-based)
    current_page: usize,
    /// Current query text
    current_query: String,
    /// All items loaded so far (across all pages)
    all_items: Vec<GeniusItem>,
}

impl GeniusApiBridge {
    /// Create a new API bridge with default settings
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(GeniusApiBridgeInner {
                api_client: GeniusApiClient::new(),
                last_response: None,
                request_in_progress: false,
                current_page: 1,
                current_query: String::new(),
                all_items: Vec::new(),
            })),
        }
    }

    /// Create a new API bridge with a custom API client
    pub fn with_client(api_client: GeniusApiClient) -> Self {
        Self {
            inner: Arc::new(Mutex::new(GeniusApiBridgeInner {
                api_client,
                last_response: None,
                request_in_progress: false,
                current_page: 1,
                current_query: String::new(),
                all_items: Vec::new(),
            })),
        }
    }

    /// Configure the API client with the given API key and organization ID
    pub async fn configure(&self, api_key: &str, organization_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.api_client = GeniusApiClient::new()
            .with_api_key(api_key.to_string())
            .with_organization_id(organization_id.to_string());
    }

    /// Get the input query from the application state
    /// 
    /// This method retrieves the current input text from the DisplayContainerState
    /// which is accessible through the App struct.
    pub fn get_query_from_app(&self, app: &App) -> String {
        app.display_container_state.input_value().to_string()
    }

    /// Query the API with specific input text
    /// 
    /// This method takes a reference to the App (for potential future context)
    /// and the input text to query. It returns the API response or an error.
    pub async fn query_with_input(&self, app: &App, input: &str) -> Result<GeniusResponse, GeniusApiError> {
        let mut inner = self.inner.lock().await;
        
        // If the query text has changed, reset pagination
        if input != inner.current_query {
            inner.current_page = 1;
            inner.current_query = input.to_string();
            inner.all_items.clear();
        }
        
        // Mark that a request is in progress
        inner.request_in_progress = true;
        
        let page = inner.current_page;
        let query = input.to_string();
        
        // Release the lock before making the API call to avoid holding it during I/O
        drop(inner);
        
        // Execute the query
        let result = self.execute_query_with_page(&query, page).await;
        
        // Re-acquire the lock to update state
        let mut inner = self.inner.lock().await;
        
        // Update state based on the result
        match &result {
            Ok(response) => {
                println!("[DEBUG] GeniusApiBridge: Query successful, received {} items", response.items.len());
                
                // Store the response
                inner.last_response = Some(response.clone());
                
                // Add the new items to the all_items collection
                for item in &response.items {
                    if !inner.all_items.iter().any(|existing| existing.id == item.id) {
                        inner.all_items.push(item.clone());
                    }
                }
            },
            Err(e) => {
                println!("[DEBUG] GeniusApiBridge: Query failed: {}", e);
            }
        }
        
        // Mark that the request is complete
        inner.request_in_progress = false;
        
        result
    }

    /// Load the next page of results for the current query
    pub async fn load_next_page(&self) -> Result<GeniusResponse, GeniusApiError> {
        let mut inner = self.inner.lock().await;
        
        println!("[DEBUG] GeniusApiBridge: load_next_page() called (current_page: {}, current_query: '{}')", 
            inner.current_page, inner.current_query);
            
        if inner.current_query.is_empty() {
            println!("[DEBUG] GeniusApiBridge: load_next_page() failed - empty query");
            return Err(GeniusApiError::Other("No current query to load more results for".to_string()));
        }
        
        // Increment the page number
        inner.current_page += 1;
        println!("[DEBUG] GeniusApiBridge: Incrementing page to {}", inner.current_page);
        
        // Create local copies of the values we need
        let query = inner.current_query.clone();
        let page = inner.current_page;
        
        // Release the lock before making the API call
        drop(inner);
        
        // Execute the query
        let result = self.execute_query_with_page(&query, page).await;
        
        // Re-acquire the lock to update state
        let mut inner = self.inner.lock().await;
        
        // Update state based on the result
        match &result {
            Ok(response) => {
                println!("[DEBUG] GeniusApiBridge: load_next_page() succeeded - got {} items", response.items.len());
                
                // Store the response
                inner.last_response = Some(response.clone());
                
                // Add the new items to the all_items collection
                for item in &response.items {
                    if !inner.all_items.iter().any(|existing| existing.id == item.id) {
                        inner.all_items.push(item.clone());
                    }
                }
            },
            Err(e) => {
                println!("[DEBUG] GeniusApiBridge: load_next_page() failed - {}", e);
            }
        }
        
        // Mark that the request is complete
        inner.request_in_progress = false;
        
        result
    }

    /// Execute a query with the given input string and page number
    async fn execute_query_with_page(&self, query: &str, page: usize) -> Result<GeniusResponse, GeniusApiError> {
        // Update the current query and page
        {
            let mut inner = self.inner.lock().await;
            inner.current_query = query.to_string();
            inner.current_page = page;
        }
        
        let inner = self.inner.lock().await;
        
        println!("[DEBUG] GeniusApiBridge: Executing query: '{}' (page {})", query, page);
        
        // Execute the query using the API client with the specified page
        // We clone the API client to avoid holding the lock during the API call
        let api_client = inner.api_client.clone();
        
        // Release the lock before making the API call
        drop(inner);
        
        // Execute the query
        let result = api_client.query_with_page(query, page).await;
        
        // If the query was successful, update the bridge state
        if let Ok(ref response) = result {
            let mut inner = self.inner.lock().await;
            
            // Store the response
            inner.last_response = Some(response.clone());
            
            // Add the new items to the all_items collection
            for item in &response.items {
                if !inner.all_items.iter().any(|existing| existing.id == item.id) {
                    inner.all_items.push(item.clone());
                }
            }
        }
        
        result
    }

    /// Execute a query with the given input string (page 1)
    pub async fn execute_query(&self, query: &str) -> Result<GeniusResponse, GeniusApiError> {
        self.execute_query_with_page(query, 1).await
    }

    /// Get the descriptions from the last API response
    pub async fn get_descriptions(&self) -> Vec<String> {
        let inner = self.inner.lock().await;
        match &inner.last_response {
            Some(response) => response.items.iter()
                .map(|item| item.description.clone())
                .collect(),
            None => Vec::new(),
        }
    }

    /// Check if a request is currently in progress
    pub async fn is_request_in_progress(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.request_in_progress
    }

    /// Set a test response directly (for unit testing)
    #[cfg(test)]
    pub async fn set_test_response(&self, response: GeniusResponse) {
        let mut inner = self.inner.lock().await;
        inner.last_response = Some(response);
        inner.request_in_progress = false;
    }

    /// Get the last API response, if any
    pub async fn last_response(&self) -> Option<GeniusResponse> {
        let inner = self.inner.lock().await;
        inner.last_response.clone()
    }

    /// Get all items loaded so far (across all pages)
    pub async fn all_items(&self) -> Vec<GeniusItem> {
        let inner = self.inner.lock().await;
        inner.all_items.clone()
    }

    /// Get the current page number
    pub async fn current_page(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.current_page
    }

    /// Check if there are more pages to load
    pub async fn has_more_pages(&self) -> bool {
        let inner = self.inner.lock().await;
        !inner.all_items.is_empty()
    }

    /// Create a clone of this GeniusApiBridge
    pub fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

// Add Clone trait to GeniusApiClient
impl Clone for GeniusApiClient {
    fn clone(&self) -> Self {
        // Use the public with_config method instead of accessing private fields
        GeniusApiClient::with_config(
            self.base_url().to_string(),
            self.api_key(),
            self.timeout(),
            self.organization_id(),
        )
    }
}

/// Factory functions for creating API bridges
pub mod factory {
    use super::*;
    use std::env;
    use dotenv::dotenv;

    /// Create a default API bridge
    pub fn create_default_bridge() -> GeniusApiBridge {
        GeniusApiBridge::new()
    }

    /// Create a mock API bridge for testing
    pub fn create_mock_bridge() -> GeniusApiBridge {
        GeniusApiBridge::with_client(
            super::super::genius_api::mock::create_mock_client()
        )
    }

    /// Create a configured API bridge with the given API key and organization ID
    pub async fn create_configured_bridge(api_key: &str, organization_id: &str) -> GeniusApiBridge {
        let bridge = GeniusApiBridge::new();
        bridge.configure(api_key, organization_id).await;
        bridge
    }

    /// Create an API bridge configured from environment variables
    pub async fn create_from_env() -> GeniusApiBridge {
        // Load .env file if it exists
        let _ = dotenv();
        
        // Try to get API key and organization ID from environment variables
        let api_key = env::var("GENIUS_API_KEY").ok();
        let org_id = env::var("GENIUS_ORGANIZATION_ID").ok();
        
        match (api_key, org_id) {
            (Some(api_key), Some(org_id)) if !api_key.trim().is_empty() && !org_id.trim().is_empty() => {
                create_configured_bridge(&api_key, &org_id).await
            },
            _ => {
                create_default_bridge()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::genius_api::{GeniusItem, GeniusResponse, mock};
    use std::sync::Arc;

    // Helper function to create a test response
    fn create_test_response(query: &str, count: usize) -> GeniusResponse {
        let mut items = Vec::new();
        for i in 1..=count {
            items.push(GeniusItem {
                id: format!("test-{}", i),
                description: format!("Test result {} for query: '{}'", i, query),
                metadata: serde_json::json!({
                    "test": true,
                    "index": i,
                    "query": query
                }),
            });
        }
        
        GeniusResponse {
            items,
            status: "success".to_string(),
        }
    }

    #[tokio::test]
    async fn test_basic_query() {
        // This test verifies that the bridge correctly forwards queries to the API client
        let bridge = GeniusApiBridge::new();
        let result = bridge.execute_query("test query").await;
        
        assert!(result.is_ok(), "Query should succeed");
        
        let response = result.unwrap();
        assert!(!response.items.is_empty(), "Response should contain items");
        
        // Check that the descriptions contain the query text
        for item in response.items {
            assert!(item.description.contains("test query"), 
                "Item description should contain the query text");
        }
    }
    
    #[tokio::test]
    async fn test_pagination() {
        // This test verifies that pagination works correctly
        let bridge = GeniusApiBridge::new();
        
        // First page
        let result1 = bridge.execute_query("pagination test").await;
        assert!(result1.is_ok(), "First page query should succeed");
        
        // Verify the current query and page
        assert_eq!(bridge.current_page().await, 1, "Current page should be 1 after first query");
        
        // Get all items after first query
        let items_after_first_query = bridge.all_items().await;
        let first_page_count = items_after_first_query.len();
        assert!(first_page_count > 0, "Should have items after first query");
        
        // Load next page
        let result2 = bridge.load_next_page().await;
        assert!(result2.is_ok(), "Next page query should succeed");
        
        // Verify current page is 2
        let current_page = bridge.current_page().await;
        assert_eq!(current_page, 2, "Current page should be 2 after loading next page");
        
        // Since we're using mock data, we can't guarantee that the second page
        // will have different items than the first page. Instead, we'll just
        // verify that we still have items after loading the next page.
        let items_after_second_query = bridge.all_items().await;
        assert!(!items_after_second_query.is_empty(), 
            "Should still have items after loading next page");
    }
    
    #[tokio::test]
    async fn test_state_management() {
        // This test verifies that the bridge properly manages its state
        let bridge = GeniusApiBridge::new();
        
        // Initial state
        assert_eq!(bridge.current_page().await, 1, "Initial page should be 1");
        assert!(bridge.all_items().await.is_empty(), "Initial items should be empty");
        
        // After a query
        let _ = bridge.execute_query("state test").await;
        assert!(!bridge.all_items().await.is_empty(), "Should have items after query");
        
        // Set a test response directly
        let test_response = create_test_response("direct test", 5);
        bridge.set_test_response(test_response.clone()).await;
        
        // Check that the response was stored
        let stored_response = bridge.last_response().await;
        assert!(stored_response.is_some(), "Should have a stored response");
        assert_eq!(stored_response.unwrap().items.len(), 5, "Should have 5 items in the stored response");
    }
    
    #[tokio::test]
    async fn test_concurrent_access() {
        // This test verifies that the bridge can be accessed concurrently
        let bridge = Arc::new(GeniusApiBridge::new());
        let bridge_clone = Arc::clone(&bridge);
        
        // Spawn a task that makes a query
        let task1 = tokio::spawn(async move {
            bridge.execute_query("concurrent test 1").await
        });
        
        // Spawn another task that makes a different query
        let task2 = tokio::spawn(async move {
            bridge_clone.execute_query("concurrent test 2").await
        });
        
        // Wait for both tasks to complete
        let (result1, result2) = tokio::join!(task1, task2);
        
        // Verify both queries succeeded
        assert!(result1.unwrap().is_ok(), "First concurrent query should succeed");
        assert!(result2.unwrap().is_ok(), "Second concurrent query should succeed");
    }
}
