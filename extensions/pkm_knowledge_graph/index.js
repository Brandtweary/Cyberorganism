/**
 * @module index
 * @description Main entry point for the Logseq Knowledge Graph Plugin
 * 
 * This module orchestrates the entire plugin functionality, connecting Logseq to a Rust-based 
 * knowledge graph backend. It handles initialization, event registration, data synchronization,
 * and communication between the Logseq frontend and the Rust backend server.
 * 
 * Key responsibilities:
 * - Plugin initialization and setup
 * - Registering Logseq slash commands for user interaction
 * - Setting up event listeners for database changes and page navigation
 * - Coordinating data processing and validation through the KnowledgeGraphDataProcessor
 * - Managing backend communication through the KnowledgeGraphAPI
 * - Implementing full database sync and incremental sync logic
 * - Handling batch processing of blocks and pages
 * - Tracking and reporting validation issues
 * 
 * Public interfaces:
 * - The plugin registers the following slash command in Logseq:
 *   - "/Check Sync Status": Checks and displays the current sync status with the backend
 * 
 * The plugin automatically:
 * - Monitors database changes via logseq.DB.onChanged
 * - Tracks page navigation via logseq.App.onRouteChanged
 * - Checks if a full sync is needed on startup
 * 
 * Dependencies:
 * - api.js: Handles all HTTP communication with the backend
 * - data_processor.js: Processes and validates Logseq data
 * - config.js: Contains configuration settings
 * 
 * @requires api
 * @requires config
 * @requires KnowledgeGraphAPI
 * @requires KnowledgeGraphDataProcessor
 */

/**
 * Logseq Knowledge Graph Plugin
 * Connects Logseq to a Rust-based knowledge graph backend
 */

// Import the API module
const api = require('./api');
// Import configuration
const config = require('./config');

// Load the API module for backend communication
// We'll use a script tag in the HTML to load api.js before this file

//=============================================================================
// LOGSEQ API INTERACTION
//=============================================================================

// Test the Logseq API connection
async function testLogseqAPI() {
  console.log('Attempting to call Logseq API...');
  try {
    const graph = await logseq.App.getCurrentGraph();
    if (graph) {
      console.log('Successfully retrieved current graph:', graph.name);
      logseq.App.showMsg(`Connected to graph: ${graph.name}`, 'success');
      return graph;
    } else {
      console.error('Failed to get current graph, API returned null.');
      logseq.App.showMsg('Failed to get graph info.', 'error');
      return null;
    }
  } catch (error) {
    console.error('Error calling Logseq API:', error);
    logseq.App.showMsg('Error interacting with Logseq API.', 'error');
    return null;
  }
}

//=============================================================================
// BACKEND COMMUNICATION
// These functions now use the global KnowledgeGraphAPI object
//=============================================================================

// Send diagnostic information to the backend server
async function sendDiagnosticInfo(message, details = {}) {
  // Use the global KnowledgeGraphAPI object's sendDiagnosticInfo function
  return KnowledgeGraphAPI.sendDiagnosticInfo(message, details);
}

// Check if backend server is available
async function checkBackendAvailability() {
  // Use the global KnowledgeGraphAPI object's checkBackendAvailability function
  return KnowledgeGraphAPI.checkBackendAvailability();
}

//=============================================================================
// DATA PROCESSING & EXTRACTION
// These functions now use the global KnowledgeGraphDataProcessor object
//=============================================================================

// Process block data and extract relevant information
async function processBlockData(block) {
  return KnowledgeGraphDataProcessor.processBlockData(block);
}

// Process page data and extract relevant information
async function processPageData(page) {
  return KnowledgeGraphDataProcessor.processPageData(page);
}

//=============================================================================
// DATA VALIDATION
// These functions now use the global KnowledgeGraphDataProcessor object
//=============================================================================

// Validate block data before sending to backend
function validateBlockData(blockData) {
  return KnowledgeGraphDataProcessor.validateBlockData(blockData);
}

// Validate page data before sending to backend
function validatePageData(pageData) {
  return KnowledgeGraphDataProcessor.validatePageData(pageData);
}

//=============================================================================
// VALIDATION ISSUE TRACKING
// Now uses the global KnowledgeGraphDataProcessor.validationIssues object
//=============================================================================

// Global validation issue tracker - reference to the one in KnowledgeGraphDataProcessor
const validationIssues = KnowledgeGraphDataProcessor.validationIssues;

//=============================================================================
// REAL-TIME SYNC HANDLING
//=============================================================================

// Process a batch of pages or blocks
async function processBatch(type, items, graphName, batchSize = 100) {
  if (!items || items.length === 0) return;
  
  console.log(`Processing ${items.length} ${type}s`);
  const batch = [];
  
  for (const item of items) {
    try {
      if (type === 'block') {
        if (!item.uuid) {
          console.warn('Skipping block: missing UUID');
          continue;
        }
        const blockData = await processBlockData(item);
        if (!blockData) {
          console.warn(`Skipping block ${item.uuid}: processing returned null`);
          continue;
        }
        const validation = validateBlockData(blockData);
        if (validation.valid) {
          batch.push(blockData);
        } else {
          console.error(`Invalid block data for UUID ${item.uuid}:`, validation.errors);
          validationIssues.addBlockIssue(blockData.id, blockData.page, validation.errors);
        }
      } else if (type === 'page') {
        if (!item.name) {
          console.warn('Skipping page: missing name');
          continue;
        }
        const pageData = await processPageData(item);
        if (!pageData) {
          console.warn(`Skipping page "${item.name}": processing returned null`);
          continue;
        }
        const validation = validatePageData(pageData);
        if (validation.valid) {
          batch.push(pageData);
        } else {
          console.error(`Invalid page data for "${item.name}":`, validation.errors);
          validationIssues.addPageIssue(pageData.name, validation.errors);
        }
      }

      if (batch.length >= batchSize) {
        await sendBatchToBackend(type, batch, graphName);
        batch.length = 0;
      }
    } catch (error) {
      const identifier = type === 'block' ? item.uuid : `"${item.name}"`;
      console.error(`Error processing ${type} ${identifier}:`, error);
    }
  }

  // Send any remaining items
  if (batch.length > 0) {
    console.log(`Sending remaining ${batch.length} ${type}s`);
    await sendBatchToBackend(type, batch, graphName);
  }
}

// Handle database changes
async function handleDBChanges(changes) {
  // Skip if no changes or empty changes array
  if (!changes || !Array.isArray(changes) || changes.length === 0) {
    return;
  }
  
  console.log(`Received ${changes.length} database changes`);
  
  // Check if backend is available before processing changes
  try {
    const backendAvailable = await checkBackendAvailability();
    if (!backendAvailable) {
      console.error('Backend server not available. Changes will not be processed.');
      return;
    }
    
    // Get current graph name
    const graph = await logseq.App.getCurrentGraph();
    if (!graph || !graph.name) {
      console.error('Failed to get current graph name.');
      return;
    }
    
    const graphName = graph.name;
    
    // Process each change
    for (const change of changes) {
      // Process block changes
      if (change.blocks && change.blocks.length > 0) {
        await processBatch('block', change.blocks, graphName, 20); // Smaller batch size for real-time
      }
      
      // Process page changes  
      if (change.pages && change.pages.length > 0) {
        await processBatch('page', change.pages, graphName, 20);
      }
    }
  } catch (error) {
    console.error('Error handling DB changes:', error);
  }
}

//=============================================================================
// FULL DATABASE SYNC
//=============================================================================

// Sync all pages and blocks in the database
async function syncFullDatabase() {
  console.log('Starting full database sync...');
  
  // Check if backend is available
  const backendAvailable = await checkBackendAvailability();
  if (!backendAvailable) {
    console.error('Backend server not available. Sync aborted.');
    logseq.App.showMsg('Backend server not available. Start the server first.', 'error');
    return false;
  }
  
  try {
    // Reset validation issues tracker
    validationIssues.reset();
    
    // Get current graph
    const graph = await logseq.App.getCurrentGraph();
    if (!graph) {
      console.error('Failed to get current graph.');
      logseq.App.showMsg('Failed to get current graph.', 'error');
      return false;
    }
    
    const graphName = graph.name;
    logseq.App.showMsg('Starting full database sync...', 'info');
    
    // Get all pages
    const allPages = await logseq.Editor.getAllPages();
    if (!allPages || !Array.isArray(allPages)) {
      console.error('Failed to fetch pages from database.');
      logseq.App.showMsg('Failed to fetch pages from database.', 'error');
      return false;
    }
    
    console.log(`Found ${allPages.length} pages to sync.`);
    
    // Track progress
    let pagesProcessed = 0;
    let blocksProcessed = 0;
    
    // Process pages in batches
    for (let i = 0; i < allPages.length; i += 100) {
      const pageBatch = allPages.slice(i, i + 100);
      
      // Skip older journal pages if there are many pages
      const filteredBatch = pageBatch.filter(page => {
        if (page.journalDay && allPages.length > 100) {
          const pageDate = new Date(page.journalDay);
          const thirtyDaysAgo = new Date();
          thirtyDaysAgo.setDate(thirtyDaysAgo.getDate() - 30);
          
          if (pageDate < thirtyDaysAgo) {
            console.log(`Skipping older journal page: ${page.name}`);
            return false;
          }
        }
        return true;
      });
      
      await processBatch('page', filteredBatch, graphName);
      pagesProcessed += filteredBatch.length;
      
      if (pagesProcessed % 10 === 0) {
        logseq.App.showMsg(`Syncing pages: ${pagesProcessed}/${allPages.length}`, 'info');
      }
      
      // Process blocks for these pages
      for (const page of filteredBatch) {
        const pageBlocksTree = await logseq.Editor.getPageBlocksTree(page.name);
        if (pageBlocksTree) {
          await processBlocksRecursively(pageBlocksTree, graphName, [], 100);
          const pageBlockCount = countBlocksInTree(pageBlocksTree);
          blocksProcessed += pageBlockCount;
          
          if (blocksProcessed % 100 === 0) {
            logseq.App.showMsg(`Processed ${blocksProcessed} blocks so far...`, 'info');
          }
        }
      }
    }

    // Display validation summary if there were issues
    const summary = validationIssues.getSummary();
    if (summary.totalBlockIssues > 0 || summary.totalPageIssues > 0) {
      console.error('Validation issues summary:', summary);
      
      // Send detailed validation summary to backend for troubleshooting
      await sendDiagnosticInfo('Validation issues summary', summary);
      
      // Show a user-friendly message with counts
      logseq.App.showMsg(
        `Sync completed with issues: ${summary.totalBlockIssues} block issues, ${summary.totalPageIssues} page issues. Check console for details.`, 
        'warning'
      );
    } else {
      // Show success message
      logseq.App.showMsg('Full database sync completed successfully!', 'success');
    }
    
    // Update sync timestamp
    await updateSyncTimestamp();
    
    // --- Summary Log ---
    // Print a summary indicating how many pages and blocks were updated and errors
    console.log('--- Logseq Knowledge Graph Sync Summary ---');
    console.log(`Pages synced: ${pagesProcessed}`);
    console.log(`Blocks synced: ${blocksProcessed}`);
    console.log(`Page errors: ${summary.totalPageIssues || 0}`);
    console.log(`Block errors: ${summary.totalBlockIssues || 0}`);
    console.log('------------------------------------------');
    // --- End Summary Log ---
    
    return true;
  } catch (error) {
    console.error('Error during full database sync:', error);
    logseq.App.showMsg('Error during full database sync. Check console for details.', 'error');
    return false;
  }
}

// Process blocks recursively with batching
async function processBlocksRecursively(blocks, graphName, blockBatch, batchSize) {
  if (!blocks || !Array.isArray(blocks)) return;
  
  for (const block of blocks) {
    try {
      // Skip blocks without UUIDs
      if (!block.uuid) {
        console.warn('Skipping block without UUID');
        continue;
      }
      
      // Process this block
      const blockData = await processBlockData(block);
      if (!blockData) {
        console.warn(`Skipping block ${block.uuid} - processing returned null`);
        continue;
      }
      
      const validation = validateBlockData(blockData);
      if (validation.valid) {
        // Add to block batch instead of sending immediately
        blockBatch.push(blockData);
        
        // Send batch if it reaches the batch size
        if (blockBatch.length >= batchSize) {
          await sendBatchToBackend('block', blockBatch, graphName);
          blockBatch.length = 0; // Reset batch more efficiently
        }
      } else {
        console.error(`Invalid block data for ${block.uuid}:`, validation.errors);
        validationIssues.addBlockIssue(blockData.id, blockData.page, validation.errors);
      }
      
      // Process children recursively
      if (block.children && block.children.length > 0) {
        await processBlocksRecursively(block.children, graphName, blockBatch, batchSize);
      }
    } catch (blockError) {
      console.error(`Error processing block ${block.uuid}:`, blockError);
      // Continue with other blocks even if one fails
    }
  }
}

// Send a batch of data to the backend
async function sendBatchToBackend(type, batch, graphName) {
  // Use the global KnowledgeGraphAPI object's sendBatchToBackend function
  return KnowledgeGraphAPI.sendBatchToBackend(type, batch, graphName);
}

// Count blocks in a tree (for progress reporting)
function countBlocksInTree(blocks) {
  if (!blocks || !Array.isArray(blocks)) return 0;
  
  let count = blocks.length;
  
  for (const block of blocks) {
    if (block.children && block.children.length > 0) {
      count += countBlocksInTree(block.children);
    }
  }
  
  return count;
}

//=============================================================================
// SYNC STATUS MANAGEMENT
//=============================================================================

// Check if a full sync is needed by querying the backend
async function checkIfFullSyncNeeded() {
  // Use the global KnowledgeGraphAPI object's checkIfFullSyncNeeded function
  return KnowledgeGraphAPI.checkIfFullSyncNeeded();
}

// Update the sync timestamp on the backend
async function updateSyncTimestamp() {
  // Use the global KnowledgeGraphAPI object's updateSyncTimestamp function
  return KnowledgeGraphAPI.updateSyncTimestamp();
}

//=============================================================================
// PLUGIN INITIALIZATION
//=============================================================================

// Main function for plugin logic
async function main() {
  console.log('Knowledge Graph Plugin initializing...');

  // Register a command to check sync status
  logseq.Editor.registerSlashCommand('Check Sync Status', async () => {
    logseq.App.showMsg('Checking sync status...', 'info');
    
    // Test backend availability
    const backendAvailable = await checkBackendAvailability();
    if (!backendAvailable) {
      logseq.App.showMsg('Backend server not available. Start the server first.', 'error');
      return;
    }
    
    // Get sync status from backend
    try {
      const response = await fetch(window.KnowledgeGraphAPI.getBackendUrl('/sync/status'), {
        method: 'GET',
        headers: {
          'Content-Type': 'application/json',
        },
      });
      
      if (!response.ok) {
        logseq.App.showMsg('Error getting sync status from backend', 'error');
        return;
      }
      
      const status = await response.json();
      
      // Display sync status
      let statusMessage = 'Sync Status:\n';
      
      if (status.last_full_sync) {
        const lastSync = new Date(status.last_full_sync);
        statusMessage += `- Last sync: ${lastSync.toLocaleString()}\n`;
        statusMessage += `- Hours since sync: ${status.hours_since_sync}\n`;
      } else {
        statusMessage += '- No previous sync detected\n';
      }
      
      statusMessage += `- Nodes: ${status.node_count}\n`;
      statusMessage += `- References: ${status.reference_count}\n`;
      statusMessage += `- Full sync needed: ${status.full_sync_needed ? 'Yes' : 'No'}`;
      
      logseq.App.showMsg(statusMessage, 'info');
    } catch (error) {
      console.error('Error checking sync status:', error);
      logseq.App.showMsg('Error checking sync status. Check console for details.', 'error');
    }
  });

  // Set up DB change monitoring
  logseq.DB.onChanged(handleDBChanges);
  
  // Listen for page open events
  logseq.App.onRouteChanged(async ({ path }) => {
    if (path.startsWith('/page/')) {
      const pageName = decodeURIComponent(path.substring(6));
      console.log(`Page opened: ${pageName}`);
      
      // You could trigger a sync here if needed
    }
  });
  
  // Check if we need to do a full sync
  console.log('Setting timeout to check for full sync in 5 seconds...');
  setTimeout(async () => {
    console.log('Timeout fired, checking if full sync is needed...');
    
    const needsFullSync = await checkIfFullSyncNeeded();
    
    if (needsFullSync) {
      logseq.App.showMsg('Performing initial database sync. This may take a while...', 'info');
      console.log('Performing initial database sync...');
      
      const success = await syncFullDatabase();
      
      if (success) {
        await updateSyncTimestamp();
        logseq.App.showMsg('Initial database sync completed successfully!', 'success');
      } else {
        logseq.App.showMsg('Initial database sync failed. Check console for details.', 'error');
      }
    } else {
      console.log('Full sync not needed');
      logseq.App.showMsg('Database is up to date. No full sync needed.', 'info');
    }
  }, 5000); // Wait 5 seconds after initialization to check for sync

  console.log('Knowledge Graph Plugin initialized. Try the /Check Sync Status command.');
}

// Initialize the plugin
logseq.ready(main).catch(console.error);
