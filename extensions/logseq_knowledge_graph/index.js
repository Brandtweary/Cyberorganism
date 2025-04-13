/**
 * Logseq Knowledge Graph Plugin
 * Connects Logseq to a Rust-based knowledge graph backend
 */

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

// Send data to the backend server
async function sendToBackend(data) {
  const backendUrl = 'http://127.0.0.1:3000/data';
  
  try {
    console.log(`Sending data to backend: ${backendUrl}`);
    const response = await fetch(backendUrl, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(data),
    });

    if (response.ok) {
      console.log('Data sent successfully to backend.');
      logseq.App.showMsg('Sent data to backend successfully!', 'success');
      return true;
    } else {
      console.error(`Backend server responded with status: ${response.status}`);
      logseq.App.showMsg(`Error sending data: Backend responded with ${response.status}`, 'error');
      return false;
    }
  } catch (error) {
    console.error('Failed to send data to backend:', error);
    logseq.App.showMsg('Failed to connect to backend server. Is it running?', 'error');
    return false;
  }
}

// Send diagnostic information to the backend server
async function sendDiagnosticInfo(message, details = {}) {
  console.log(`DIAGNOSTIC: ${message}`, details);
  
  try {
    const graph = await logseq.App.getCurrentGraph();
    await sendToBackend({
      source: 'Diagnostic',
      timestamp: new Date().toISOString(),
      graphName: graph ? graph.name : 'unknown',
      type_: 'diagnostic',
      payload: JSON.stringify({
        message,
        details
      })
    });
  } catch (error) {
    console.error('Error sending diagnostic info:', error);
  }
}

// Check if backend server is available
async function checkBackendAvailability() {
  console.log('Checking backend server availability...');
  try {
    const response = await fetch('http://127.0.0.1:3000/', {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
      },
    });
    
    const available = response.ok;
    console.log(`Backend server available: ${available}`);
    
    // Only send diagnostic if server is available
    if (available) {
      await sendDiagnosticInfo('Backend availability check', { available });
    }
    
    return available;
  } catch (error) {
    console.error('Backend server not available:', error);
    return false;
  }
}

// Extract all references from content using regex
function extractReferencesFromContent(content) {
  if (!content) return [];
  
  const references = [];
  
  // Extract page references [[Page Name]]
  const pageRefRegex = /\[\[(.*?)\]\]/g;
  let match;
  while ((match = pageRefRegex.exec(content)) !== null) {
    references.push({
      type: 'page',
      name: match[1].trim()
    });
  }
  
  // Extract block references ((block-id))
  const blockRefRegex = /\(\((.*?)\)\)/g;
  while ((match = blockRefRegex.exec(content)) !== null) {
    references.push({
      type: 'block',
      id: match[1].trim()
    });
  }
  
  // Extract hashtags #tag
  const tagRegex = /#([a-zA-Z0-9_-]+)/g;
  while ((match = tagRegex.exec(content)) !== null) {
    // Don't include the # symbol in the tag name
    references.push({
      type: 'tag',
      name: match[1].trim()
    });
  }
  
  // Extract properties key:: value
  const propRegex = /([a-zA-Z0-9_-]+)::\s*(.*?)($|\n)/g;
  while ((match = propRegex.exec(content)) !== null) {
    const propName = match[1].trim();
    const propValue = match[2].trim();
    
    // The property name itself is a reference
    references.push({
      type: 'property',
      name: propName
    });
    
    // Check if the property value contains references
    // We don't need to extract these since they'll be caught by the other regex patterns
    // when we process the full content
  }
  
  return references;
}

// Process block data and extract relevant information
async function processBlockData(block) {
  try {
    // Get full block content and metadata with includeChildren option
    const blockEntity = await logseq.Editor.getBlock(block.uuid, { includeChildren: true });
    if (!blockEntity) {
      console.error(`Failed to get block with UUID: ${block.uuid}`);
      return null;
    }
    
    // Filter out empty blocks - they don't belong in a knowledge graph
    if (!blockEntity.content || blockEntity.content.trim() === '') {
      const pageName = blockEntity.page ? 
        (await logseq.Editor.getPage(blockEntity.page.id))?.name || 'unknown' : 
        'unknown';
      
      validationIssues.addBlockIssue(block.uuid, pageName, ['Empty block content - skipped']);
      return null; // Skip this block entirely
    }
    
    // Get the page that contains this block
    const page = blockEntity.page ? await logseq.Editor.getPage(blockEntity.page.id) : null;
    
    // Extract all references from the content using our unified regex approach
    const references = extractReferencesFromContent(blockEntity.content);
    
    // Get parent UUID instead of parent ID
    let parentUUID = null;
    if (blockEntity.parent) {
      // If parent is an object with a uuid property, use that
      if (blockEntity.parent.uuid) {
        parentUUID = blockEntity.parent.uuid;
      } 
      // If we only have the parent ID, try to get the block to get its UUID
      else if (blockEntity.parent.id) {
        try {
          const parentBlock = await logseq.Editor.getBlock(blockEntity.parent.id, { includeChildren: true });
          if (parentBlock) {
            parentUUID = parentBlock.uuid;
          }
        } catch (e) {
          // Only log actual errors
          console.warn(`Could not resolve parent ID ${blockEntity.parent.id} to UUID for block ${blockEntity.uuid}`);
        }
      }
    }
    
    return {
      id: blockEntity.uuid,
      content: blockEntity.content,
      created: blockEntity.created || new Date().toISOString(),
      updated: blockEntity.updated || new Date().toISOString(),
      parent: parentUUID,
      children: blockEntity.children ? blockEntity.children.map(child => 
        typeof child === 'object' && child.uuid ? child.uuid : 
        typeof child === 'string' ? child : null
      ).filter(Boolean) : [],
      page: page ? page.name : null,
      properties: blockEntity.properties || {},
      references: references
    };
  } catch (error) {
    // Only log actual errors
    console.error('Error processing block data:', error);
    return null;
  }
}

// Process page data and extract relevant information
async function processPageData(page) {
  try {
    // Skip pages without names
    if (!page.name || page.name.trim() === '') {
      validationIssues.addPageIssue('unknown', ['Nameless page - skipped']);
      return null; // Skip this page entirely
    }
    
    // Get page properties and metadata
    const pageEntity = await logseq.Editor.getPage(page.name);
    if (!pageEntity) {
      console.error(`Failed to get page with name: ${page.name}`);
      return null;
    }
    
    // Get all blocks in the page
    const blocks = await logseq.Editor.getPageBlocksTree(page.name);
    const blockIds = blocks ? blocks.map(block => block.uuid) : [];
    
    // Get page properties
    const properties = pageEntity.properties || {};
    
    return {
      name: page.name,
      created: pageEntity.created || new Date().toISOString(),
      updated: pageEntity.updated || new Date().toISOString(),
      properties: properties,
      blocks: blockIds
    };
  } catch (error) {
    console.error('Error processing page data:', error);
    return null;
  }
}

// Validate block data before sending to backend
function validateBlockData(blockData) {
  if (!blockData) {
    console.error('Block data is null or undefined');
    return { valid: false, errors: ['Block data is null or undefined'] };
  }
  
  const errors = [];
  
  // Check required fields
  if (!blockData.id || typeof blockData.id !== 'string') {
    errors.push(`Invalid block ID: ${blockData.id}`);
  }
  
  // Check for missing or empty content
  if (blockData.content === undefined) {
    errors.push('Missing block content field');
  } else if (blockData.content === null || blockData.content.trim() === '') {
    errors.push('Block content is empty');
  }
  
  // Validate created/updated timestamps
  if (!blockData.created || typeof blockData.created !== 'string') {
    errors.push(`Invalid created timestamp: ${blockData.created}`);
  }
  
  if (!blockData.updated || typeof blockData.updated !== 'string') {
    errors.push(`Invalid updated timestamp: ${blockData.updated}`);
  }
  
  // Validate parent (should be null or string UUID)
  if (blockData.parent !== null && typeof blockData.parent !== 'string') {
    errors.push(`Invalid parent reference: ${blockData.parent}`);
  }
  
  // Validate children (should be array of string UUIDs)
  if (!Array.isArray(blockData.children)) {
    errors.push(`Children is not an array: ${blockData.children}`);
  } else {
    for (let i = 0; i < blockData.children.length; i++) {
      const child = blockData.children[i];
      if (typeof child !== 'string') {
        errors.push(`Invalid child reference at index ${i}: ${child}`);
      }
    }
  }
  
  // Validate references
  if (!Array.isArray(blockData.references)) {
    errors.push(`References is not an array: ${blockData.references}`);
  } else {
    for (let i = 0; i < blockData.references.length; i++) {
      const ref = blockData.references[i];
      if (!ref.type) {
        errors.push(`Missing reference type at index ${i}`);
      }
    }
  }
  
  return { 
    valid: errors.length === 0,
    errors: errors
  };
}

// Validate page data before sending to backend
function validatePageData(pageData) {
  if (!pageData) {
    console.error('Page data is null or undefined');
    return { valid: false, errors: ['Page data is null or undefined'] };
  }
  
  const errors = [];
  
  // Check required fields
  if (!pageData.name || typeof pageData.name !== 'string') {
    errors.push(`Invalid page name: ${pageData.name}`);
  }
  
  // Validate created/updated timestamps
  if (!pageData.created || typeof pageData.created !== 'string') {
    errors.push(`Invalid created timestamp: ${pageData.created}`);
  }
  
  if (!pageData.updated || typeof pageData.updated !== 'string') {
    errors.push(`Invalid updated timestamp: ${pageData.updated}`);
  }
  
  // Validate blocks (should be array of string UUIDs)
  if (!Array.isArray(pageData.blocks)) {
    errors.push(`Blocks is not an array: ${pageData.blocks}`);
  } else {
    for (let i = 0; i < pageData.blocks.length; i++) {
      const block = pageData.blocks[i];
      if (typeof block !== 'string') {
        errors.push(`Invalid block reference at index ${i}: ${block}`);
      }
    }
  }
  
  return { 
    valid: errors.length === 0,
    errors: errors
  };
}

// Global validation issue tracker
const validationIssues = {
  blocks: {},
  pages: {},
  totalBlockIssues: 0,
  totalPageIssues: 0,
  
  // Add a block validation issue
  addBlockIssue(blockId, pageName, issues) {
    if (!this.blocks[pageName]) {
      this.blocks[pageName] = [];
    }
    this.blocks[pageName].push({ blockId, issues });
    this.totalBlockIssues++;
  },
  
  // Add a page validation issue
  addPageIssue(pageName, issues) {
    if (!this.pages[pageName]) {
      this.pages[pageName] = [];
    }
    this.pages[pageName].push({ issues });
    this.totalPageIssues++;
  },
  
  // Get a summary of all validation issues
  getSummary() {
    const summary = {
      totalBlockIssues: this.totalBlockIssues,
      totalPageIssues: this.totalPageIssues,
      blockIssuesByPage: {},
      pageIssues: {}
    };
    
    // Summarize block issues by page
    for (const pageName in this.blocks) {
      summary.blockIssuesByPage[pageName] = this.blocks[pageName].length;
    }
    
    // Summarize page issues
    for (const pageName in this.pages) {
      summary.pageIssues[pageName] = this.pages[pageName].length;
    }
    
    return summary;
  },
  
  // Reset the tracker
  reset() {
    this.blocks = {};
    this.pages = {};
    this.totalBlockIssues = 0;
    this.totalPageIssues = 0;
  }
};

// Handle database changes
async function handleDBChanges(changes) {
  try {
    console.log('DB changes detected:', changes.length);
    
    // Process each change
    for (const change of changes) {
      // Skip non-block and non-page changes
      if (!change.blocks && !change.pages) continue;
      
      // Process block changes
      if (change.blocks && change.blocks.length > 0) {
        for (const block of change.blocks) {
          console.log(`Processing changed block: ${block.uuid}`);
          
          const blockData = await processBlockData(block);
          if (blockData) {
            const validation = validateBlockData(blockData);
            if (validation.valid) {
              const graph = await logseq.App.getCurrentGraph();
              await sendToBackend({
                source: 'Logseq DB Change',
                timestamp: new Date().toISOString(),
                graphName: graph ? graph.name : 'unknown',
                type_: 'block',
                payload: JSON.stringify(blockData)
              });
            } else {
              console.error('Invalid block data:', validation.errors);
              validationIssues.addBlockIssue(blockData.id, blockData.page, validation.errors);
              await sendDiagnosticInfo('Invalid block data', { 
                errors: validation.errors,
                blockData: blockData
              });
            }
          }
        }
      }
      
      // Process page changes
      if (change.pages && change.pages.length > 0) {
        for (const page of change.pages) {
          console.log(`Processing changed page: ${page.name}`);
          
          const pageData = await processPageData(page);
          if (pageData) {
            const validation = validatePageData(pageData);
            if (validation.valid) {
              const graph = await logseq.App.getCurrentGraph();
              await sendToBackend({
                source: 'Logseq DB Change',
                timestamp: new Date().toISOString(),
                graphName: graph ? graph.name : 'unknown',
                type_: 'page',
                payload: JSON.stringify(pageData)
              });
            } else {
              console.error('Invalid page data:', validation.errors);
              validationIssues.addPageIssue(pageData.name, validation.errors);
              await sendDiagnosticInfo('Invalid page data', { 
                errors: validation.errors,
                pageData: pageData
              });
            }
          }
        }
      }
    }
  } catch (error) {
    console.error('Error handling DB changes:', error);
  }
}

// Sync all pages and blocks in the database
async function syncFullDatabase() {
  try {
    // Reset validation issues tracker
    validationIssues.reset();
    
    const graph = await logseq.App.getCurrentGraph();
    if (!graph) {
      console.error('Failed to get current graph.');
      return false;
    }
    
    logseq.App.showMsg('Starting full database sync...', 'info');
    console.log('Starting full database sync...');
    
    // Get all pages
    const allPages = await logseq.Editor.getAllPages();
    console.log(`Found ${allPages.length} pages to sync.`);
    
    // Track progress
    let pagesProcessed = 0;
    let blocksProcessed = 0;
    
    // Process each page
    for (const page of allPages) {
      // Skip journal pages if they're too numerous
      if (page.journalDay && allPages.length > 100) {
        // Only process recent journal pages (last 30 days)
        const pageDate = new Date(page.journalDay);
        const thirtyDaysAgo = new Date();
        thirtyDaysAgo.setDate(thirtyDaysAgo.getDate() - 30);
        
        if (pageDate < thirtyDaysAgo) {
          console.log(`Skipping older journal page: ${page.name}`);
          continue;
        }
      }
      
      // Process page
      const pageData = await processPageData(page);
      if (pageData) {
        const validation = validatePageData(pageData);
        if (validation.valid) {
          await sendToBackend({
            source: 'Full Sync',
            timestamp: new Date().toISOString(),
            graphName: graph.name,
            type_: 'page',
            payload: JSON.stringify(pageData)
          });
        } else {
          console.error('Invalid page data:', validation.errors);
          validationIssues.addPageIssue(pageData.name, validation.errors);
          await sendDiagnosticInfo('Invalid page data', { 
            errors: validation.errors,
            pageData: pageData
          });
        }
        
        pagesProcessed++;
        
        // Show progress every 10 pages
        if (pagesProcessed % 10 === 0) {
          logseq.App.showMsg(`Syncing pages: ${pagesProcessed}/${allPages.length}`, 'info');
        }
      }
      
      // Get all blocks for this page
      const pageBlocksTree = await logseq.Editor.getPageBlocksTree(page.name);
      
      // Process blocks recursively
      await processBlocksRecursively(pageBlocksTree, graph.name);
      
      // Update blocks processed count
      blocksProcessed += countBlocksInTree(pageBlocksTree);
      
      // Show progress for blocks every 100 blocks
      if (blocksProcessed % 100 === 0) {
        logseq.App.showMsg(`Processed ${blocksProcessed} blocks so far...`, 'info');
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
    
    return true;
  } catch (error) {
    console.error('Error during full database sync:', error);
    logseq.App.showMsg('Error during full database sync. Check console for details.', 'error');
    return false;
  }
}

// Process blocks recursively
async function processBlocksRecursively(blocks, graphName) {
  if (!blocks || !Array.isArray(blocks)) return;
  
  for (const block of blocks) {
    // Process this block
    const blockData = await processBlockData(block);
    if (blockData) {
      const validation = validateBlockData(blockData);
      if (validation.valid) {
        await sendToBackend({
          source: 'Full Sync',
          timestamp: new Date().toISOString(),
          graphName: graphName,
          type_: 'block',
          payload: JSON.stringify(blockData)
        });
      } else {
        // Use the validation issues tracker instead of verbose logging
        validationIssues.addBlockIssue(blockData.id, blockData.page || 'unknown', validation.errors);
      }
    }
    
    // Process children recursively
    if (block.children && block.children.length > 0) {
      await processBlocksRecursively(block.children, graphName);
    }
  }
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

// Check if a full sync is needed by querying the backend
async function checkIfFullSyncNeeded() {
  console.log('Checking if full sync is needed...');
  try {
    // Check if backend is available
    const backendAvailable = await checkBackendAvailability();
    if (!backendAvailable) {
      console.log('Backend not available, skipping full sync check');
      return false;
    }
    
    // Query the backend for sync status
    const response = await fetch('http://127.0.0.1:3000/sync/status', {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
      },
    });
    
    if (!response.ok) {
      console.error('Error getting sync status from backend');
      return false;
    }
    
    const status = await response.json();
    console.log('Sync status from backend:', status);
    
    // Send diagnostic info about sync status
    await sendDiagnosticInfo('Sync status from backend', status);
    
    // Return whether a full sync is needed
    return status.full_sync_needed === true;
  } catch (error) {
    console.error('Error checking if full sync is needed:', error);
    await sendDiagnosticInfo('Error checking if full sync needed', { 
      error: error.message,
      stack: error.stack
    });
    return false;
  }
}

// Update the sync timestamp on the backend
async function updateSyncTimestamp() {
  try {
    const response = await fetch('http://127.0.0.1:3000/sync/update', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
    });
    
    if (!response.ok) {
      console.error('Error updating sync timestamp on backend');
      return false;
    }
    
    const result = await response.json();
    console.log('Sync timestamp updated:', result);
    
    // Send diagnostic info about timestamp update
    await sendDiagnosticInfo('Sync timestamp updated', result);
    
    return result.success === true;
  } catch (error) {
    console.error('Error updating sync timestamp:', error);
    await sendDiagnosticInfo('Error updating sync timestamp', { 
      error: error.message,
      stack: error.stack
    });
    return false;
  }
}

// Main function for plugin logic
async function main() {
  console.log('Knowledge Graph Plugin initializing...');

  // Register a command to test reference extraction
  logseq.Editor.registerSlashCommand('Test References', async () => {
    const currentBlock = await logseq.Editor.getCurrentBlock();
    if (!currentBlock) {
      logseq.App.showMsg('No current block found.', 'warning');
      return;
    }
    
    // Get the full block entity
    const blockEntity = await logseq.Editor.getBlock(currentBlock.uuid);
    console.log('Block entity:', blockEntity);
    
    // Extract references using our unified regex method
    console.log('--- REFERENCE EXTRACTION ---');
    const references = extractReferencesFromContent(blockEntity.content);
    console.log('References extracted:', references);
    
    // Display a summary to the user
    if (references.length > 0) {
      const summary = `Found ${references.length} references:
        - Page refs: ${references.filter(r => r.type === 'page').length}
        - Block refs: ${references.filter(r => r.type === 'block').length}
        - Tags: ${references.filter(r => r.type === 'tag').length}
        - Properties: ${references.filter(r => r.type === 'property').length}`;
      
      logseq.App.showMsg(summary, 'info');
      
      // Send the references to the backend for analysis
      const graph = await logseq.App.getCurrentGraph();
      await sendToBackend({
        source: 'Test References Command',
        timestamp: new Date().toISOString(),
        graphName: graph ? graph.name : 'unknown',
        type_: 'test_references',  
        payload: JSON.stringify({
          blockId: blockEntity.uuid,
          content: blockEntity.content,
          references: references
        })
      });
    } else {
      logseq.App.showMsg('No references found in this block.', 'info');
    }
  });

  // Register a command to manually sync the current page
  logseq.Editor.registerSlashCommand('Sync Current Page', async () => {
    const currentPage = await logseq.Editor.getCurrentPage();
    if (!currentPage) {
      logseq.App.showMsg('No current page found.', 'warning');
      return;
    }
    
    logseq.App.showMsg(`Syncing page: ${currentPage.name}...`, 'info');
    
    const pageData = await processPageData(currentPage);
    if (pageData) {
      const validation = validatePageData(pageData);
      if (validation.valid) {
        const graph = await logseq.App.getCurrentGraph();
        await sendToBackend({
          source: 'Manual Sync',
          timestamp: new Date().toISOString(),
          graphName: graph ? graph.name : 'unknown',
          type_: 'page',
          payload: JSON.stringify(pageData)
        });
      } else {
        console.error('Invalid page data:', validation.errors);
        validationIssues.addPageIssue(pageData.name, validation.errors);
        await sendDiagnosticInfo('Invalid page data', { 
          errors: validation.errors,
          pageData: pageData
        });
      }
      
      // Get all blocks for this page
      const pageBlocksTree = await logseq.Editor.getPageBlocksTree(currentPage.name);
      
      // Process blocks recursively
      await processBlocksRecursively(pageBlocksTree, graph ? graph.name : 'unknown');
      
      logseq.App.showMsg(`Page ${currentPage.name} synced successfully!`, 'success');
    } else {
      logseq.App.showMsg(`Failed to sync page ${currentPage.name}.`, 'error');
    }
  });
  
  // Register a command to perform a full database sync
  logseq.Editor.registerSlashCommand('Full Database Sync', async () => {
    const backendAvailable = await checkBackendAvailability();
    if (!backendAvailable) {
      logseq.App.showMsg('Backend server not available. Start the server first.', 'error');
      return;
    }
    
    logseq.App.showMsg('Starting full database sync. This may take a while...', 'info');
    
    const success = await syncFullDatabase();
    
    if (success) {
      await updateSyncTimestamp();
      logseq.App.showMsg('Full database sync completed successfully!', 'success');
    } else {
      logseq.App.showMsg('Full database sync failed. Check console for details.', 'error');
    }
  });

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
      const response = await fetch('http://127.0.0.1:3000/sync/status', {
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
    
    // Send diagnostic that timeout fired
    await sendDiagnosticInfo('Sync check timeout fired', { 
      time: new Date().toISOString() 
    });
    
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

  console.log('Knowledge Graph Plugin initialized. Try the /Test KG Plugin, /Sync Current Page, or /Full Database Sync commands.');
}

// Initialize the plugin
logseq.ready(main).catch(console.error);
