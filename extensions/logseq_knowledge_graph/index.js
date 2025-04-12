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

  console.log('Knowledge Graph Plugin initialized. Try the /Test KG Plugin command.');
}

// Initialize the plugin
logseq.ready(main).catch(console.error);
