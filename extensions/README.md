# Cyberorganism Extensions

This directory contains extensions and modifications to the original AIChat codebase for the Cyberorganism fork.

## Purpose

The extensions folder is designed to keep our changes contained and clearly separated from the original codebase, making it easier to:

1. Identify what has been modified from the original AIChat project
2. Maintain compatibility with upstream changes
3. Document and organize our custom features

## Structure

The extensions directory is organized as follows:

- `README.md` - This documentation file
- `pkm_knowledge_graph/` - Integration between a Personal Knowledge Management app (PKM) and a knowledge graph
- (Future subdirectories will be added as needed for specific extension categories)

## Extensions

### PKM Knowledge Graph

The `pkm_knowledge_graph` module provides integration between a PKM app and a knowledge graph. It consists of:

- A Logseq plugin that exports block and page data
- A Rust backend server that receives and stores the data
- (Planned) Integration with petgraph for knowledge graph construction

For testing purposes, we use a `logseq_dummy_graph` instance that contains sample data.

#### Configuration

The extension uses its own configuration file separate from the main AIChat configuration:

1. Copy `extensions/pkm_knowledge_graph/config.example.yaml` to `extensions/pkm_knowledge_graph/config.yaml`
2. Edit the settings as needed:
   ```yaml
   backend:
     host: 127.0.0.1     # Host to bind the server to
     port: 3000          # Default port for the backend server
     max_port_attempts: 10  # Number of alternative ports to try if default is busy
   ```

This configuration is used by both the JavaScript frontend and Rust backend to ensure consistent settings. If the default port is unavailable, the server will automatically try the next available port.

Note: The `config.yaml` file is ignored by git to allow for local customization without affecting the repository.

## Development Guidelines

When adding new features to Cyberorganism:

1. Try to keep changes to the original codebase minimal
2. Place new functionality in this extensions directory when possible
3. Document any changes made to the original AIChat codebase
