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
- `logseq_knowledge_graph/` - Integration between Logseq PKM and a knowledge graph
- (Future subdirectories will be added as needed for specific extension categories)

## Extensions

### Logseq Knowledge Graph

The `logseq_knowledge_graph` module provides integration between Logseq (a personal knowledge management system) and a graph database. It consists of:

- A Logseq plugin that exports block and page data
- A Rust backend server that receives and stores the data
- (Planned) Integration with petgraph for knowledge graph construction

For testing purposes, we use a `logseq_dummy_graph` instance that contains sample data.

## Development Guidelines

When adding new features to Cyberorganism:

1. Try to keep changes to the original codebase minimal
2. Place new functionality in this extensions directory when possible
3. Document any changes made to the original AIChat codebase
