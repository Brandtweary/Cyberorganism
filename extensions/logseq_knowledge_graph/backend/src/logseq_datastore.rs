use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use thiserror::Error;

/// Errors that can occur when working with the Logseq datastore
#[derive(Error, Debug)]
pub enum DatastoreError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    // NodeNotFound variant is never constructed. Remove it to resolve the warning.
    // NodeNotFound(String),
    
    #[error("Reference resolution error: {0}")]
    ReferenceResolution(String),
    
    #[error("Failed to parse datetime: {0}")]
    DateTimeParseError(#[from] chrono::ParseError),
}

/// Result type for datastore operations
pub type DatastoreResult<T> = Result<T, DatastoreError>;

/// Type of node in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeType {
    /// A Logseq page
    Page,
    
    /// A Logseq block
    Block,
}

/// Represents a node in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Our internal unique identifier
    pub id: String,
    
    /// Original Logseq identifier (UUID for blocks, name for pages)
    pub logseq_id: String,
    
    /// Type of node (Page or Block)
    pub node_type: NodeType,
    
    /// Content of the node (block content or page name)
    pub content: String,
    
    /// When the node was created
    pub created_at: DateTime<Utc>,
    
    /// When the node was last updated
    pub updated_at: DateTime<Utc>,
    
    /// Parent node ID (if any)
    pub parent_id: Option<String>,
    
    /// Child node IDs
    pub children: Vec<String>,
    
    /// Properties associated with this node (key-value pairs)
    pub properties: HashMap<String, String>,
}

/// Type of reference between nodes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReferenceType {
    /// Reference to a page: [[Page Name]]
    PageRef,
    
    /// Reference to a block: ((block-id))
    BlockRef,
    
    /// Tag reference: #tag
    Tag,
    
    /// Property reference: key:: value
    Property,
    
    /// Parent-child relationship
    ParentChild,
}

/// Represents a directional reference between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    /// ID of the source node (where the reference originates)
    pub source_id: String,
    
    /// ID of the target node (what is being referenced)
    pub target_id: String,
    
    /// Type of reference
    pub ref_type: ReferenceType,
}

/// Metadata about the datastore
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DatastoreMetadata {
    /// When the last full sync was performed (Unix timestamp in milliseconds)
    last_full_sync: Option<i64>,
    
    /// Number of nodes in the datastore
    node_count: usize,
    
    /// Number of references in the datastore
    reference_count: usize,
    
    /// When the datastore was created
    created_at: Option<i64>,
    
    /// When the datastore was last updated
    updated_at: Option<i64>,
}

/// Represents the complete datastore state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct DatastoreState {
    /// All nodes in the knowledge graph
    nodes: HashMap<String, Node>,
    
    /// All references between nodes
    references: Vec<Reference>,
    
    /// Mapping from Logseq block UUIDs to our internal node IDs
    block_id_map: HashMap<String, String>,
    
    /// Mapping from Logseq page names to our internal node IDs
    page_name_map: HashMap<String, String>,
    
    /// Metadata about the datastore
    #[serde(default)]
    metadata: DatastoreMetadata,
}

/// Manages the storage and retrieval of knowledge graph data
pub struct LogseqDatastore {
    /// Base directory for storing data
    data_dir: PathBuf,
    
    /// Complete state of the datastore
    state: DatastoreState,
}

impl LogseqDatastore {
    /// Create a new datastore with the given data directory
    pub fn new<P: AsRef<Path>>(data_dir: P) -> DatastoreResult<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        
        // Create the data directory if it doesn't exist
        fs::create_dir_all(&data_dir)?;
        
        let mut datastore = Self {
            data_dir,
            state: DatastoreState::default(),
        };
        
        // Try to load existing state, but don't fail if it doesn't exist
        match datastore.load_state() {
            Ok(loaded_state) => {
                println!("Loaded existing datastore state");
                datastore.state = loaded_state;
                
                // Log metadata for debugging
                if let Some(last_sync) = datastore.state.metadata.last_full_sync {
                    let dt = DateTime::<Utc>::from_timestamp_millis(last_sync)
                        .unwrap_or_else(|| Utc::now());
                    println!("Found last sync timestamp: {} ({})", last_sync, dt.to_rfc3339());
                } else {
                    println!("No last sync timestamp found in loaded state");
                }
            },
            Err(e) => {
                println!("Error loading datastore state: {:?}", e);
            }
        }
        
        // Initialize metadata if this is a new datastore
        if datastore.state.metadata.created_at.is_none() {
            println!("Initializing new datastore");
            let now = Utc::now().timestamp_millis();
            datastore.state.metadata.created_at = Some(now);
            datastore.state.metadata.updated_at = Some(now);
            let _ = datastore.save_state();
        }
        
        Ok(datastore)
    }
    
    /// Load the complete datastore state from disk
    pub fn load_state(&self) -> DatastoreResult<DatastoreState> {
        let state_path = self.data_dir.join("datastore.json");
        
        if !state_path.exists() {
            return Ok(DatastoreState::default());
        }
        
        let mut file = File::open(state_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        
        let state: DatastoreState = serde_json::from_str(&contents)?;
        Ok(state)
    }
    
    /// Save the complete datastore state to disk
    pub fn save_state(&self) -> DatastoreResult<()> {
        let state_path = self.data_dir.join("datastore.json");
        let json = serde_json::to_string_pretty(&self.state)?;
        
        let mut file = File::create(state_path)?;
        file.write_all(json.as_bytes())?;
        
        Ok(())
    }
    
    // pub fn get_node(&self, id: &str) -> DatastoreResult<&Node> {
    //     self.state.nodes.get(id)
    //         .ok_or_else(|| DatastoreError::NodeNotFound(id.to_string()))
    // }
    
    // pub fn get_node_by_logseq_block_id(&self, logseq_id: &str) -> DatastoreResult<&Node> {
    //     let node_id = self.state.block_id_map.get(logseq_id)
    //         .ok_or_else(|| DatastoreError::NodeNotFound(format!("Block UUID: {}", logseq_id)))?;
        
    //     self.get_node(node_id)
    // }
    
    // pub fn get_node_by_logseq_page_name(&self, page_name: &str) -> DatastoreResult<&Node> {
    //     let node_id = self.state.page_name_map.get(page_name)
    //         .ok_or_else(|| DatastoreError::NodeNotFound(format!("Page name: {}", page_name)))?;
        
    //     self.get_node(node_id)
    // }
    
    // pub fn get_all_nodes(&self) -> Vec<&Node> {
    //     self.state.nodes.values().collect()
    // }
    
    // pub fn get_all_references(&self) -> &[Reference] {
    //     &self.state.references
    // }
    
    // pub fn get_outgoing_references(&self, node_id: &str) -> Vec<&Reference> {
    //     self.state.references.iter()
    //         .filter(|r| r.source_id == node_id)
    //         .collect()
    // }
    
    // pub fn get_incoming_references(&self, node_id: &str) -> Vec<&Reference> {
    //     self.state.references.iter()
    //         .filter(|r| r.target_id == node_id)
    //         .collect()
    // }
    
    /// Create or update a node from Logseq block data
    pub fn create_or_update_node_from_logseq_block(&mut self, block_data: &LogseqBlockData) -> DatastoreResult<String> {
        let logseq_id = &block_data.id;
        let node_id = if let Some(existing_id) = self.state.block_id_map.get(logseq_id) {
            existing_id.clone()
        } else {
            Uuid::new_v4().to_string()
        };
        
        // Create or update the node
        let node = Node {
            id: node_id.clone(),
            logseq_id: logseq_id.clone(),
            node_type: NodeType::Block,
            content: block_data.content.clone(),
            created_at: parse_datetime(&block_data.created)?,
            updated_at: parse_datetime(&block_data.updated)?,
            parent_id: block_data.parent.clone(),
            children: block_data.children.clone(),
            properties: parse_properties(&block_data.properties),
        };
        
        // Update our mappings
        self.state.nodes.insert(node_id.clone(), node);
        self.state.block_id_map.insert(logseq_id.clone(), node_id.clone());
        
        // Process parent-child relationships
        if let Some(parent_id) = &block_data.parent {
            // If parent is a block, resolve to our internal ID
            if let Some(parent_node_id) = self.state.block_id_map.get(parent_id) {
                let parent_node_id_clone = parent_node_id.clone();
                
                // Add parent-child reference
                self.add_reference(Reference {
                    source_id: parent_node_id_clone.clone(),
                    target_id: node_id.clone(),
                    ref_type: ReferenceType::ParentChild,
                });
                
                // Update parent node's children list
                if let Some(parent_node) = self.state.nodes.get_mut(&parent_node_id_clone) {
                    if !parent_node.children.contains(&node_id) {
                        parent_node.children.push(node_id.clone());
                    }
                }
            }
        }
        
        // Process page relationship
        if let Some(page_name) = &block_data.page {
            // Ensure the page exists in our datastore
            let page_node_id = self.ensure_page_exists(page_name)?;
            
            // If this is a root block (no parent), add it to the page's children
            if block_data.parent.is_none() {
                if let Some(page_node) = self.state.nodes.get_mut(&page_node_id) {
                    if !page_node.children.contains(&node_id) {
                        page_node.children.push(node_id.clone());
                    }
                }
            }
        }
        
        // Process references
        for reference in &block_data.references {
            self.resolve_and_add_reference(&node_id, reference)?;
        }
        
        // Save the updated state
        self.save_state()?;
        
        Ok(node_id)
    }
    
    /// Create or update a node from Logseq page data
    pub fn create_or_update_node_from_logseq_page(&mut self, page_data: &LogseqPageData) -> DatastoreResult<String> {
        let page_name = &page_data.name;
        let node_id = if let Some(existing_id) = self.state.page_name_map.get(page_name) {
            existing_id.clone()
        } else {
            Uuid::new_v4().to_string()
        };
        
        // Create or update the node
        let node = Node {
            id: node_id.clone(),
            logseq_id: page_name.clone(),
            node_type: NodeType::Page,
            content: page_name.clone(),
            created_at: parse_datetime(&page_data.created)?,
            updated_at: parse_datetime(&page_data.updated)?,
            parent_id: None,
            children: page_data.blocks.clone(),
            properties: parse_properties(&page_data.properties),
        };
        
        // Update our mappings
        self.state.nodes.insert(node_id.clone(), node);
        self.state.page_name_map.insert(page_name.clone(), node_id.clone());
        
        // Process root blocks
        for block_id in &page_data.blocks {
            if let Some(block_node_id) = self.state.block_id_map.get(block_id) {
                // Add parent-child reference
                self.add_reference(Reference {
                    source_id: node_id.clone(),
                    target_id: block_node_id.clone(),
                    ref_type: ReferenceType::ParentChild,
                });
            }
        }
        
        // Save the updated state
        self.save_state()?;
        
        Ok(node_id)
    }
    
    /// Ensure a page exists in our datastore, creating it if necessary
    fn ensure_page_exists(&mut self, page_name: &str) -> DatastoreResult<String> {
        if let Some(page_id) = self.state.page_name_map.get(page_name) {
            return Ok(page_id.clone());
        }
        
        // Page doesn't exist, create a placeholder
        let node_id = Uuid::new_v4().to_string();
        
        let node = Node {
            id: node_id.clone(),
            logseq_id: page_name.to_string(),
            node_type: NodeType::Page,
            content: page_name.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            parent_id: None,
            children: Vec::new(),
            properties: HashMap::new(),
        };
        
        self.state.nodes.insert(node_id.clone(), node);
        self.state.page_name_map.insert(page_name.to_string(), node_id.clone());
        
        Ok(node_id)
    }
    
    /// Add a reference to the datastore
    fn add_reference(&mut self, reference: Reference) {
        // Check if this reference already exists
        let exists = self.state.references.iter().any(|r| 
            r.source_id == reference.source_id && 
            r.target_id == reference.target_id && 
            r.ref_type == reference.ref_type
        );
        
        if !exists {
            self.state.references.push(reference);
        }
    }
    
    /// Resolve a Logseq reference to our internal IDs and add it to the datastore
    fn resolve_and_add_reference(&mut self, source_node_id: &str, reference: &LogseqReference) -> DatastoreResult<()> {
        match reference.r#type.as_str() {
            "page" => {
                // Ensure the referenced page exists
                let target_node_id = self.ensure_page_exists(&reference.name)?;
                
                // Add the reference
                self.add_reference(Reference {
                    source_id: source_node_id.to_string(),
                    target_id: target_node_id,
                    ref_type: ReferenceType::PageRef,
                });
            },
            "block" => {
                // Check if we know about this block
                if let Some(target_node_id) = self.state.block_id_map.get(&reference.id) {
                    // Add the reference
                    self.add_reference(Reference {
                        source_id: source_node_id.to_string(),
                        target_id: target_node_id.clone(),
                        ref_type: ReferenceType::BlockRef,
                    });
                } else {
                    // Block doesn't exist in our system yet, create a placeholder
                    let target_node_id = Uuid::new_v4().to_string();
                    
                    let node = Node {
                        id: target_node_id.clone(),
                        logseq_id: reference.id.clone(),
                        node_type: NodeType::Block,
                        content: "".to_string(), // Placeholder content
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                        parent_id: None,
                        children: Vec::new(),
                        properties: HashMap::new(),
                    };
                    
                    self.state.nodes.insert(target_node_id.clone(), node);
                    self.state.block_id_map.insert(reference.id.clone(), target_node_id.clone());
                    
                    // Add the reference
                    self.add_reference(Reference {
                        source_id: source_node_id.to_string(),
                        target_id: target_node_id,
                        ref_type: ReferenceType::BlockRef,
                    });
                }
            },
            "tag" => {
                // Tags are treated as special pages
                let tag_name = format!("#{}", reference.name);
                let target_node_id = self.ensure_page_exists(&tag_name)?;
                
                // Add the reference
                self.add_reference(Reference {
                    source_id: source_node_id.to_string(),
                    target_id: target_node_id,
                    ref_type: ReferenceType::Tag,
                });
            },
            "property" => {
                // Properties could be handled in various ways
                // For now, we'll just store them in the node's properties map
                // and not create explicit references
                // This could be expanded in the future
            },
            _ => {
                return Err(DatastoreError::ReferenceResolution(
                    format!("Unknown reference type: {}", reference.r#type)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Check if a full sync is needed based on time since last sync
    pub fn is_full_sync_needed(&self) -> bool {
        let now = Utc::now().timestamp_millis();
        
        match self.state.metadata.last_full_sync {
            None => {
                println!("Full sync needed: No previous sync found");
                true
            },
            Some(last_sync) => {
                let hours_since_sync = (now - last_sync) / (1000 * 60 * 60);
                let full_sync_needed = hours_since_sync > 2;
                
                println!("Last sync: {}, Hours since sync: {}, Full sync needed: {}", 
                         last_sync, hours_since_sync, full_sync_needed);
                
                full_sync_needed
            }
        }
    }
    
    /// Get the current sync status
    pub fn get_sync_status(&self) -> serde_json::Value {
        let now = Utc::now().timestamp_millis();
        let hours_since_sync = self.state.metadata.last_full_sync.map(|last_sync| {
            let hours = (now - last_sync) / (1000 * 60 * 60);
            hours
        });
        
        serde_json::json!({
            "created_at": self.state.metadata.created_at.unwrap_or(0),
            "updated_at": self.state.metadata.updated_at.unwrap_or(0),
            "last_full_sync": self.state.metadata.last_full_sync,
            "hours_since_sync": hours_since_sync,
            "full_sync_needed": self.is_full_sync_needed(),
            "node_count": self.state.nodes.len(),
            "reference_count": self.state.references.len(),
        })
    }
    
    /// Update the last full sync timestamp
    pub fn update_full_sync_timestamp(&mut self) -> DatastoreResult<()> {
        let now = Utc::now().timestamp_millis();
        self.state.metadata.last_full_sync = Some(now);
        self.state.metadata.updated_at = Some(now);
        
        // Update node and reference counts
        self.state.metadata.node_count = self.state.nodes.len();
        self.state.metadata.reference_count = self.state.references.len();
        
        self.save_state()
    }
}

/// Data structures for Logseq data received from the plugin

#[derive(Debug, Clone, Deserialize)]
pub struct LogseqBlockData {
    pub id: String,
    pub content: String,
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub created: String,
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub updated: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default)]
    pub page: Option<String>,
    #[serde(default)]
    pub properties: serde_json::Value,
    #[serde(default)]
    pub references: Vec<LogseqReference>,
}

impl LogseqBlockData {
    /// Validate the block data to ensure it meets our requirements
    pub fn validate(&self) -> Result<(), String> {
        let mut errors = Vec::new();
        
        // Check required fields
        if self.id.is_empty() {
            errors.push("Block ID is empty".to_string());
        }
        
        if self.content.is_empty() {
            errors.push("Block content is empty".to_string());
        }
        
        // Check created/updated timestamps
        if self.created.is_empty() {
            errors.push("Created timestamp is empty".to_string());
        }
        
        if self.updated.is_empty() {
            errors.push("Updated timestamp is empty".to_string());
        }
        
        // Validate parent (should be None or non-empty string)
        if let Some(parent) = &self.parent {
            if parent.is_empty() {
                errors.push("Parent ID is empty".to_string());
            }
        }
        
        // Return result
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join(", "))
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogseqPageData {
    pub name: String,
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub created: String,
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub updated: String,
    #[serde(default)]
    pub properties: serde_json::Value,
    #[serde(default)]
    pub blocks: Vec<String>,
}

impl LogseqPageData {
    /// Validate the page data to ensure it meets our requirements
    pub fn validate(&self) -> Result<(), String> {
        let mut errors = Vec::new();
        
        // Check required fields
        if self.name.is_empty() {
            errors.push("Page name is empty".to_string());
        }
        
        // Check created/updated timestamps
        if self.created.is_empty() {
            errors.push("Created timestamp is empty".to_string());
        }
        
        if self.updated.is_empty() {
            errors.push("Updated timestamp is empty".to_string());
        }
        
        // Return result
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join(", "))
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogseqReference {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub id: String,
}

// Custom deserializer for timestamps that can be either strings or integers
fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct TimestampVisitor;

    impl<'de> serde::de::Visitor<'de> for TimestampVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or an integer")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value.to_string())
        }
    }

    deserializer.deserialize_any(TimestampVisitor)
}

// Helper functions

/// Parse a datetime string from Logseq
fn parse_datetime(datetime_str: &str) -> DatastoreResult<DateTime<Utc>> {
    // Try parsing with different formats
    if let Ok(dt) = DateTime::parse_from_rfc3339(datetime_str) {
        return Ok(dt.with_timezone(&Utc));
    }
    
    // Try ISO 8601 format
    if let Ok(dt) = DateTime::parse_from_str(datetime_str, "%Y-%m-%dT%H:%M:%S%.fZ") {
        return Ok(dt.with_timezone(&Utc));
    }
    
    // Try Unix timestamp (milliseconds)
    if let Ok(timestamp) = datetime_str.parse::<i64>() {
        // Handle both millisecond and second timestamps
        let timestamp_millis = if timestamp > 1_000_000_000_000 {
            // Already in milliseconds
            timestamp
        } else {
            // Convert seconds to milliseconds
            timestamp * 1000
        };
        
        if let Some(dt) = DateTime::from_timestamp_millis(timestamp_millis) {
            return Ok(dt);
        }
    }
    
    // If all parsing attempts fail, log the issue and use current time
    println!("Warning: Could not parse datetime '{}', using current time", datetime_str);
    Ok(Utc::now())
}

/// Parse properties from a JSON value
fn parse_properties(properties_json: &serde_json::Value) -> HashMap<String, String> {
    let mut properties = HashMap::new();
    
    if let Some(obj) = properties_json.as_object() {
        for (key, value) in obj {
            if let Some(value_str) = value.as_str() {
                properties.insert(key.clone(), value_str.to_string());
            } else {
                properties.insert(key.clone(), value.to_string());
            }
        }
    }
    
    properties
}
