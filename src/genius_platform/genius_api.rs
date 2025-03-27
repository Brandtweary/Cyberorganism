#![allow(dead_code)]
#![allow(unused_variables)]

//! Genius API client implementation
//!
//! # Schema Update Instructions
//!
//! When the actual Genius API schema becomes available, the following components
//! in this file will need to be updated:
//!
//! 1. **Data Structures**:
//!    - Update `GeniusItem` struct to match the actual response item format
//!    - Update `GeniusResponse` struct to match the actual response envelope
//!    - Add any additional data structures needed for the API
//!
//! 2. **Request Construction**:
//!    - In the `query` method, update the request body JSON
//!    - Ensure all required fields are included in the request
//!    - Update headers if needed (currently using Bearer token authentication)
//!
//! 3. **Response Parsing**:
//!    - Ensure the response parsing logic correctly handles the actual API format
//!    - Update error handling for any API-specific error responses
//!
//! 4. **Mock Data**:
//!    - Update the `mock_query` method to return data that matches the structure
//!      of the real API responses for testing purposes
//!
//! The rest of the application interacts with this API through the `GeniusApiBridge`,
//! so changes should be contained to this file and won't affect other parts of the
//! application as long as the public interface remains consistent.

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;
use uuid::Uuid;

/// Represents an item returned from the Genius API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeniusItem {
    /// Unique identifier for the item
    pub id: String,
    /// Description text for the item
    pub description: String,
    /// Additional metadata as a JSON object
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Response from the Genius API containing multiple items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeniusResponse {
    /// List of items returned from the API
    pub items: Vec<GeniusItem>,
    /// Status of the response
    pub status: String,
}

/// Error types that can occur during API operations
#[derive(Debug)]
pub enum GeniusApiError {
    /// Error occurred during network request
    NetworkError(String),
    /// Error parsing the response
    ParseError(String),
    /// API returned an error
    ApiError(String),
    /// Other unexpected errors
    Other(String),
}

impl std::fmt::Display for GeniusApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NetworkError(msg) => write!(f, "Network error: {msg}"),
            Self::ParseError(msg) => write!(f, "Parse error: {msg}"),
            Self::ApiError(msg) => write!(f, "API error: {msg}"),
            Self::Other(msg) => write!(f, "Other error: {msg}"),
        }
    }
}

impl Error for GeniusApiError {}

/// Client for interacting with the Genius API
pub struct GeniusApiClient {
    base_url: String,
    api_key: Option<String>,
    timeout: Duration,
    organization_id: String,
    session_id: String,
    http_client: reqwest::Client,
}

impl GeniusApiClient {
    /// Create a new API client with default settings
    pub fn new() -> Self {
        // Create a reqwest client with default timeout
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
            
        Self {
            base_url: "https://app.productgenius.io".to_string(),
            api_key: None,
            timeout: Duration::from_secs(10),
            organization_id: String::new(),
            session_id: Uuid::new_v4().to_string(),
            http_client,
        }
    }

    /// Create a new API client with custom configuration
    pub fn with_config(
        base_url: String,
        api_key: Option<String>,
        timeout: Duration,
        organization_id: String,
    ) -> Self {
        // Create a reqwest client with the specified timeout
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();
            
        Self {
            base_url,
            api_key,
            timeout,
            organization_id,
            session_id: Uuid::new_v4().to_string(),
            http_client,
        }
    }

    /// Set the API key
    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    /// Set the organization ID
    pub fn with_organization_id(mut self, organization_id: String) -> Self {
        self.organization_id = organization_id;
        self
    }

    /// Get the base URL for the API
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the timeout duration
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Get the API key if available
    pub fn api_key(&self) -> Option<String> {
        self.api_key.clone()
    }
    
    /// Get the organization ID
    pub fn organization_id(&self) -> String {
        self.organization_id.clone()
    }

    /// Get the server URL for API requests
    fn get_server_url(&self) -> String {
        format!("{}/hackathon/{}/feed/{}", 
            self.base_url, 
            self.organization_id,
            self.session_id
        )
    }

    /// Query the API asynchronously with a specific page number
    pub async fn query_with_page(&self, input: &str, page: usize) -> Result<GeniusResponse, GeniusApiError> {
        // When mock-api feature is explicitly enabled, always use mock data
        #[cfg(feature = "mock-api")]
        {
            Ok(self.mock_query(input, page))
        }

        // In normal mode, try to use real API but fall back to mock if no API key or organization ID
        #[cfg(not(feature = "mock-api"))]
        {
            // If no API key is provided or it's empty, or organization ID is empty, fall back to mock data
            if self.api_key.as_ref().is_none_or(|k| k.trim().is_empty()) ||
               self.organization_id.is_empty() {
                Ok(self.mock_query(input, page))
            } else {
                // API key is available, proceed with real API request
                let api_key = self.api_key.as_ref().unwrap();
                let server_url = self.get_server_url();
                
                // Prepare the request body based on the genius-hackathon-skeleton implementation
                let request_body = serde_json::json!({
                    "search_prompt": input,
                    "page": page,
                    "batch_count": 10
                });
                
                // Make the async request
                let response = match self.http_client.post(&server_url)
                    .header("Content-Type", "application/json")
                    .header("Authorization", format!("Bearer {api_key}"))
                    .json(&request_body)
                    .send()
                    .await {
                        Ok(response) => response,
                        Err(e) => return Err(GeniusApiError::NetworkError(e.to_string())),
                    };
                
                // Check if the request was successful
                if !response.status().is_success() {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                    return Err(GeniusApiError::ApiError(format!("API returned error: {status} - {error_text}")));
                }
                
                // Parse the response
                let response_text = match response.text().await {
                    Ok(text) => text,
                    Err(e) => return Err(GeniusApiError::NetworkError(e.to_string())),
                };
                
                // Try to parse the response as JSON
                let response_json: serde_json::Value = match serde_json::from_str(&response_text) {
                    Ok(json) => json,
                    Err(e) => return Err(GeniusApiError::ParseError(e.to_string())),
                };
                
                // Extract the cards from the response
                if let Some(cards) = response_json.get("cards") {
                    match self.convert_cards_to_items(cards) {
                        Ok(items) => {
                            return Ok(GeniusResponse {
                                items,
                                status: "success".to_string(),
                            });
                        },
                        Err(e) => return Err(e),
                    }
                }
                
                Err(GeniusApiError::ParseError("Response does not contain 'cards' field".to_string()))
            }
        }
    }

    /// Query the API asynchronously (page 1)
    pub async fn query(&self, input: &str) -> Result<GeniusResponse, GeniusApiError> {
        self.query_with_page(input, 1).await
    }

    /// Convert cards from the API response to `GeniusItems`
    #[allow(clippy::unused_self)]
    fn convert_cards_to_items(&self, cards: &serde_json::Value) -> Result<Vec<GeniusItem>, GeniusApiError> {
        if let Some(cards_array) = cards.as_array() {
            let mut items = Vec::new();
            
            for card in cards_array {
                // Extract the card ID
                let id = match card.get("id") {
                    Some(id) => id.as_str().unwrap_or("unknown").to_string(),
                    None => Uuid::new_v4().to_string(), // Generate a random ID if none is provided
                };
                
                // Extract the card content
                let description = match card.get("content") {
                    Some(content) => content.as_str().unwrap_or("").to_string(),
                    None => continue, // Skip cards without content
                };
                
                // Skip empty descriptions
                if description.trim().is_empty() {
                    continue;
                }
                
                // Create a GeniusItem from the card
                items.push(GeniusItem {
                    id,
                    description,
                    metadata: card.clone(),
                });
            }
            
            Ok(items)
        } else {
            Err(GeniusApiError::ParseError("Cards is not an array".to_string()))
        }
    }

    /// Create a mock response for testing and development
    #[allow(clippy::unused_self)]
    fn mock_query(&self, query: &str, page: usize) -> GeniusResponse {
        let mut items = Vec::new();
        
        // Calculate the starting index based on the page number
        let start_idx = (page - 1) * 10 + 1;
        let end_idx = start_idx + 9;
        
        // Generate 10 mock items with the query text
        for i in start_idx..=end_idx {
            let id = format!("mock-{i}");
            let description = format!("Mock result {i} for query: '{query}' (page {page})");
            
            items.push(GeniusItem {
                id,
                description,
                metadata: serde_json::json!({
                    "relevance": 0.9 - ((i % 10) as f64 * 0.05),
                    "source": "mock-data",
                    "query": query,
                    "page": page,
                }),
            });
        }
        
        GeniusResponse {
            items,
            status: "success".to_string(),
        }
    }
}

/// Module containing mock implementations for testing
pub mod mock {
    use super::*;
    
    /// Creates a mock API client that returns predefined responses
    pub fn create_mock_client() -> GeniusApiClient {
        GeniusApiClient::new()
    }
    
    /// Creates a mock response with the given items
    pub fn create_mock_response(items: Vec<GeniusItem>) -> GeniusResponse {
        GeniusResponse {
            items,
            status: "success".to_string(),
        }
    }
}

/// Utility functions for working with API responses
pub mod utils {
    use super::*;
    
    /// Extract descriptions from a list of items
    pub fn extract_descriptions(response: &GeniusResponse) -> Vec<String> {
        response.items.iter()
            .map(|item| item.description.clone())
            .collect()
    }
}
