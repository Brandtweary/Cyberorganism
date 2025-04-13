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
    // Get full block content and metadata
    const blockEntity = await logseq.Editor.getBlock(block.uuid);
    if (!blockEntity) return null;
    
    // Get the page that contains this block
    const page = blockEntity.page ? await logseq.Editor.getPage(blockEntity.page.id) : null;
    
    // Extract all references from the content using our unified regex approach
    const references = extractReferencesFromContent(blockEntity.content);
    
    return {
      id: blockEntity.uuid,
      content: blockEntity.content,
      created: blockEntity.created || new Date().toISOString(),
      updated: blockEntity.updated || new Date().toISOString(),
      parent: blockEntity.parent ? blockEntity.parent.id : null,
      children: blockEntity.children || [],
      page: page ? page.name : null,
      properties: blockEntity.properties || {},
      references: references
    };
  } catch (error) {
    console.error('Error processing block data:', error);
    return null;
  }
}

// Process page data and extract relevant information
async function processPageData(page) {
  try {
    // Get full page content and metadata
    const pageEntity = await logseq.Editor.getPage(page.uuid);
    if (!pageEntity) return null;
    
    // Get root blocks of the page
    const blocks = await logseq.Editor.getPageBlocksTree(pageEntity.name);
    
    // We'll extract references from each block individually when we process them
    // No need to use getPageLinkedReferences since we're already parsing all blocks
    
    return {
      name: pageEntity.name,
      created: pageEntity.created || new Date().toISOString(),
      updated: pageEntity.updated || new Date().toISOString(),
      properties: pageEntity.properties || {},
      blocks: blocks ? blocks.map(b => b.uuid) : []
    };
  } catch (error) {
    console.error('Error processing page data:', error);
    return null;
  }
}

// Handle database changes
async function handleDBChanges(changes) {
  console.log('DB changes detected:', changes);
  
  try {
    const graph = await logseq.App.getCurrentGraph();
    if (!graph) return;
    
    // Extract blocks that were changed
    const changedBlocks = changes.blocks || [];
    
    // Process each changed block
    for (const block of changedBlocks) {
      const processedBlock = await processBlockData(block);
      if (processedBlock) {
        await sendToBackend({
          source: 'Logseq DB Change',
          timestamp: new Date().toISOString(),
          graphName: graph.name,
          type: 'block',
          payload: JSON.stringify(processedBlock)
        });
      }
    }
    
    // If we have transaction metadata, we can determine what kind of operation occurred
    if (changes.txMeta && changes.txMeta.outlinerOp) {
      console.log('Operation type:', changes.txMeta.outlinerOp);
      
      // If a page was created or modified, we might want to process it separately
      if (changes.txMeta.outlinerOp === 'savePage' && changes.txMeta.pageId) {
        const pageData = await processPageData({ uuid: changes.txMeta.pageId });
        if (pageData) {
          await sendToBackend({
            source: 'Logseq DB Change',
            timestamp: new Date().toISOString(),
            graphName: graph.name,
            type: 'page',
            payload: JSON.stringify(pageData)
          });
        }
      }
    }
  } catch (error) {
    console.error('Error handling DB changes:', error);
  }
}

// Main function for plugin logic
function main() {
  console.log('Knowledge Graph Plugin initializing...');

  // Register a simple slash command for testing
  logseq.Editor.registerSlashCommand('Test KG Plugin', async () => {
    logseq.App.showMsg('Knowledge Graph Plugin Test command executed!');
    const graph = await testLogseqAPI();
    
    if (graph) {
      // Send test data to backend
      const dummyData = {
        source: 'Logseq Plugin Slash Command',
        timestamp: new Date().toISOString(),
        graphName: graph.name,
        payload: 'This is a test message.',
      };
      
      await sendToBackend(dummyData);
    }
  });
  
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
      const graph = await logseq.App.getCurrentGraph();
      await sendToBackend({
        source: 'Manual Sync',
        timestamp: new Date().toISOString(),
        graphName: graph ? graph.name : 'unknown',
        type: 'page',
        payload: JSON.stringify(pageData)
      });
      
      logseq.App.showMsg(`Page ${currentPage.name} synced successfully!`, 'success');
    } else {
      logseq.App.showMsg(`Failed to sync page ${currentPage.name}.`, 'error');
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

  console.log('Knowledge Graph Plugin initialized. Try the /Test KG Plugin or /Sync Current Page commands.');
}

// Initialize the plugin
logseq.ready(main).catch(console.error);
