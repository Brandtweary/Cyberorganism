/**
 * Logseq Knowledge Graph Plugin - API Module
 * Handles all communication with the Rust backend server
 */

// Create a global API object to hold all the functions
window.KnowledgeGraphAPI = {};

/**
 * Send data to the backend server
 * @param {Object} data - Data to send to the backend
 * @returns {Promise<boolean>} - Whether the data was sent successfully
 */
window.KnowledgeGraphAPI.sendToBackend = async function(data) {
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

/**
 * Send diagnostic information to the backend server
 * @param {string} message - Diagnostic message
 * @param {Object} details - Additional details
 */
window.KnowledgeGraphAPI.sendDiagnosticInfo = async function(message, details = {}) {
  console.log(`DIAGNOSTIC: ${message}`, details);
  
  try {
    const graph = await logseq.App.getCurrentGraph();
    await window.KnowledgeGraphAPI.sendToBackend({
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

/**
 * Check if backend server is available
 * @returns {Promise<boolean>} - Whether the backend server is available
 */
window.KnowledgeGraphAPI.checkBackendAvailability = async function() {
  console.log('Checking backend server availability...');
  try {
    const response = await fetch('http://127.0.0.1:3000/', {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
      },
    });
    
    return response.ok;
  } catch (error) {
    console.error('Error checking backend availability:', error);
    return false;
  }
}

/**
 * Check if a full sync is needed by querying the backend
 * @returns {Promise<boolean>} - Whether a full sync is needed
 */
window.KnowledgeGraphAPI.checkIfFullSyncNeeded = async function() {
  console.log('Checking if full sync is needed...');
  try {
    // Check if backend is available
    const backendAvailable = await window.KnowledgeGraphAPI.checkBackendAvailability();
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
    
    // Return whether a full sync is needed
    return status.full_sync_needed === true;
  } catch (error) {
    console.error('Error checking if full sync is needed:', error);
    await window.KnowledgeGraphAPI.sendDiagnosticInfo('Error checking if full sync needed', { 
      error: error.message,
      stack: error.stack
    });
    return false;
  }
}

/**
 * Update the sync timestamp on the backend
 * @returns {Promise<boolean>} - Whether the update was successful
 */
window.KnowledgeGraphAPI.updateSyncTimestamp = async function() {
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
    
    return result.success === true;
  } catch (error) {
    console.error('Error updating sync timestamp:', error);
    await window.KnowledgeGraphAPI.sendDiagnosticInfo('Error updating sync timestamp', { 
      error: error.message,
      stack: error.stack
    });
    return false;
  }
}

/**
 * Send a batch of data to the backend
 * @param {string} type - Type of data (block or page)
 * @param {Array} batch - Array of data items
 * @param {string} graphName - Name of the graph
 */
window.KnowledgeGraphAPI.sendBatchToBackend = async function(type, batch, graphName) {
  if (batch.length === 0) return;
  
  console.log(`Sending batch of ${batch.length} ${type}s to backend`);
  
  try {
    await window.KnowledgeGraphAPI.sendToBackend({
      source: 'Full Sync',
      timestamp: new Date().toISOString(),
      graphName: graphName,
      type_: `${type}_batch`,
      payload: JSON.stringify(batch)
    });
  } catch (error) {
    console.error(`Error sending ${type} batch:`, error);
  }
}
