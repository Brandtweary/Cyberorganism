/**
 * Configuration loader for the Logseq Knowledge Graph plugin
 */

// Import required modules
const fs = require('fs');
const path = require('path');
const yaml = require('js-yaml');

// Default configuration
const defaultConfig = {
  backend: {
    host: '127.0.0.1',
    port: 3000,
    max_port_attempts: 10
  }
};

/**
 * Load configuration from config.yaml file
 * @returns {Object} Configuration object
 */
function loadConfig() {
  try {
    // Try to find config.yaml in the current directory or parent directories
    const configPath = findConfigFile();
    
    if (configPath) {
      const configContent = fs.readFileSync(configPath, 'utf8');
      const config = yaml.load(configContent);
      console.log('Loaded configuration from', configPath);
      return config;
    }
  } catch (error) {
    console.error('Error loading configuration:', error);
  }
  
  // Return default configuration if loading fails
  console.log('Using default configuration');
  return defaultConfig;
}

/**
 * Find the config.yaml file in the current directory or parent directories
 * @returns {string|null} Path to the config file or null if not found
 */
function findConfigFile() {
  // Start with the current directory
  let currentDir = __dirname;
  
  // Check up to 3 parent directories
  for (let i = 0; i < 4; i++) {
    const configPath = path.join(currentDir, 'config.yaml');
    
    if (fs.existsSync(configPath)) {
      return configPath;
    }
    
    // Move up to parent directory
    const parentDir = path.dirname(currentDir);
    
    // If we've reached the root directory, stop searching
    if (parentDir === currentDir) {
      break;
    }
    
    currentDir = parentDir;
  }
  
  return null;
}

// Export the configuration
const config = loadConfig();
module.exports = config;
