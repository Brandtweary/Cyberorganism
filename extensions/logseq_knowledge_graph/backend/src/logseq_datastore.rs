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
    
    #[error("Block not found: {0}")]
    BlockNotFound(String),
    
    #[error("Page not found: {0}")]
    PageNotFound(String),
}

/// Result type for datastore operations
pub type DatastoreResult<T> = Result<T, DatastoreError>;

/// Represents a Logseq block with its content and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Unique identifier for the block
    pub id: String,
    
    /// The actual content of the block
    pub content: String,
    
    /// When the block was created
    pub created_at: DateTime<Utc>,
    
    /// When the block was last updated
    pub updated_at: DateTime<Utc>,
    
    /// ID of the parent block (if any)
    pub parent_id: Option<String>,
    
    /// IDs of child blocks
    pub children: Vec<String>,
    
    /// Page that contains this block
    pub page: Option<String>,
    
    /// Block properties (key-value pairs)
    pub properties: HashMap<String, String>,
    
    /// References to other blocks
    pub references: Vec<String>,
}

/// Represents a Logseq page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    /// Name of the page
    pub name: String,
    
    /// When the page was created
    pub created_at: DateTime<Utc>,
    
    /// When the page was last updated
    pub updated_at: DateTime<Utc>,
    
    /// Properties of the page
    pub properties: HashMap<String, String>,
    
    /// Root blocks in the page
    pub blocks: Vec<String>,
}

/// Manages the storage and retrieval of Logseq data
pub struct LogseqDatastore {
    /// Base directory for storing data
    data_dir: PathBuf,
    
    /// In-memory cache of blocks
    blocks: HashMap<String, Block>,
    
    /// In-memory cache of pages
    pages: HashMap<String, Page>,
}

impl LogseqDatastore {
    /// Create a new datastore with the given data directory
    pub fn new<P: AsRef<Path>>(data_dir: P) -> DatastoreResult<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        
        // Create the data directory if it doesn't exist
        fs::create_dir_all(&data_dir)?;
        
        // Create subdirectories for blocks and pages
        let blocks_dir = data_dir.join("blocks");
        let pages_dir = data_dir.join("pages");
        
        fs::create_dir_all(&blocks_dir)?;
        fs::create_dir_all(&pages_dir)?;
        
        Ok(Self {
            data_dir,
            blocks: HashMap::new(),
            pages: HashMap::new(),
        })
    }
    
    /// Load all data from disk
    pub fn load_all(&mut self) -> DatastoreResult<()> {
        self.load_blocks()?;
        self.load_pages()?;
        Ok(())
    }
    
    /// Load all blocks from disk
    fn load_blocks(&mut self) -> DatastoreResult<()> {
        let blocks_dir = self.data_dir.join("blocks");
        
        for entry in fs::read_dir(blocks_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                let mut file = File::open(&path)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                
                let block: Block = serde_json::from_str(&contents)?;
                self.blocks.insert(block.id.clone(), block);
            }
        }
        
        Ok(())
    }
    
    /// Load all pages from disk
    fn load_pages(&mut self) -> DatastoreResult<()> {
        let pages_dir = self.data_dir.join("pages");
        
        for entry in fs::read_dir(pages_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                let mut file = File::open(&path)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                
                let page: Page = serde_json::from_str(&contents)?;
                self.pages.insert(page.name.clone(), page);
            }
        }
        
        Ok(())
    }
    
    /// Save a block to the datastore
    pub fn save_block(&mut self, block: Block) -> DatastoreResult<()> {
        // Update in-memory cache
        let block_id = block.id.clone();
        self.blocks.insert(block_id.clone(), block.clone());
        
        // Save to disk
        let block_path = self.data_dir.join("blocks").join(format!("{}.json", block_id));
        let json = serde_json::to_string_pretty(&block)?;
        
        let mut file = File::create(block_path)?;
        file.write_all(json.as_bytes())?;
        
        Ok(())
    }
    
    /// Save a page to the datastore
    pub fn save_page(&mut self, page: Page) -> DatastoreResult<()> {
        // Update in-memory cache
        let page_name = page.name.clone();
        self.pages.insert(page_name.clone(), page.clone());
        
        // Save to disk
        let page_path = self.data_dir.join("pages").join(format!("{}.json", page_name));
        let json = serde_json::to_string_pretty(&page)?;
        
        let mut file = File::create(page_path)?;
        file.write_all(json.as_bytes())?;
        
        Ok(())
    }
    
    /// Get a block by ID
    pub fn get_block(&self, id: &str) -> DatastoreResult<&Block> {
        self.blocks.get(id).ok_or_else(|| DatastoreError::BlockNotFound(id.to_string()))
    }
    
    /// Get a page by name
    pub fn get_page(&self, name: &str) -> DatastoreResult<&Page> {
        self.pages.get(name).ok_or_else(|| DatastoreError::PageNotFound(name.to_string()))
    }
    
    /// Get all blocks
    pub fn get_all_blocks(&self) -> Vec<&Block> {
        self.blocks.values().collect()
    }
    
    /// Get all pages
    pub fn get_all_pages(&self) -> Vec<&Page> {
        self.pages.values().collect()
    }
    
    /// Create a new block
    pub fn create_block(&mut self, content: String, parent_id: Option<String>, page: Option<String>) -> DatastoreResult<Block> {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        
        let block = Block {
            id,
            content,
            created_at: now,
            updated_at: now,
            parent_id,
            children: Vec::new(),
            page,
            properties: HashMap::new(),
            references: Vec::new(),
        };
        
        self.save_block(block.clone())?;
        
        // Update parent block if it exists
        if let Some(parent_id) = &block.parent_id {
            if let Ok(parent) = self.get_block(parent_id).cloned() {
                let mut updated_parent = parent;
                updated_parent.children.push(block.id.clone());
                updated_parent.updated_at = now;
                self.save_block(updated_parent)?;
            }
        }
        
        // Update page if it exists
        if let Some(page_name) = &block.page {
            if let Ok(page) = self.get_page(page_name).cloned() {
                let mut updated_page = page;
                
                // Only add to root blocks if it has no parent
                if block.parent_id.is_none() {
                    updated_page.blocks.push(block.id.clone());
                }
                
                updated_page.updated_at = now;
                self.save_page(updated_page)?;
            }
        }
        
        Ok(block)
    }
    
    /// Create a new page
    pub fn create_page(&mut self, name: String) -> DatastoreResult<Page> {
        let now = Utc::now();
        
        let page = Page {
            name,
            created_at: now,
            updated_at: now,
            properties: HashMap::new(),
            blocks: Vec::new(),
        };
        
        self.save_page(page.clone())?;
        Ok(page)
    }
}

// Helper function to parse Logseq block references and other special syntax
pub fn parse_references(content: &str) -> Vec<String> {
    // This is a placeholder for more sophisticated parsing
    // In a real implementation, we'd use regex or a proper parser
    
    let mut references = Vec::new();
    
    // Simple detection of ((block-id)) references
    // This is just a basic implementation - would need to be more robust
    let mut i = 0;
    while i < content.len() {
        if content[i..].starts_with("((") {
            let start = i + 2;
            if let Some(end) = content[start..].find("))") {
                let reference = &content[start..(start + end)];
                references.push(reference.to_string());
                i = start + end + 2;
            } else {
                i += 2;
            }
        } else {
            i += 1;
        }
    }
    
    references
}
