# Structurizr Architecture Diagrams

**Status (2026-09-29):** `architecture.dsl` is a **pre-decoupling in-process engine view** (TUI-centric, Binance-centric, no `strategy_core` / three-repo split, no Paper/Demo qualification). It is not current system status and not a production or mainnet diagram. Use [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md).

This document explains how to use the Structurizr architecture diagram for the Trading System.

## Overview

The Structurizr DSL file (`architecture.dsl` in this directory) defines a comprehensive architecture model of the Trading System, including:

- **System Context**: High-level view of the system and its external dependencies
- **Container View**: Major containers (services) within the Trading System
- **Component Views**: Detailed component views for each service
- **Deployment View**: Production deployment architecture
- **Styles**: Color-coded styling for different element types

## Prerequisites

To use the Structurizr diagram, you need:

1. **Structurizr CLI** or **Structurizr Lite** (free, local version)
2. **Java 11+** (required for Structurizr)

### Installation

#### Option 1: Structurizr Lite (Recommended for Local Use)

```bash
# Download Structurizr Lite
curl -L https://github.com/structurizr/lite/releases/latest/download/structurizr-lite.zip -o structurizr-lite.zip
unzip structurizr-lite.zip
cd structurizr-lite
```

#### Option 2: Structurizr CLI

```bash
# Download Structurizr CLI
curl -L https://github.com/structurizr/cli/releases/latest/download/structurizr-cli.zip -o structurizr-cli.zip
unzip structurizr-cli.zip
```

## Usage

### Using Structurizr Lite (Web Interface)

1. **Start Structurizr Lite**:
   ```bash
   java -jar structurizr-lite.jar
   ```

2. **Open in Browser**:
   - Navigate to `http://localhost:8080`
   - Click "Create workspace"
   - Paste the contents of `architecture/architecture.dsl` (or navigate to this directory) into the editor
   - Click "Update"

3. **View Diagrams**:
   - System Context
   - Container diagrams
   - Component diagrams
   - Deployment diagrams

### Using Structurizr CLI (Export Diagrams)

```bash
# Export all diagrams as PNG (from project root)
java -jar structurizr-cli.jar export -workspace architecture/architecture.dsl -format plantuml

# Or export to other formats
java -jar structurizr-cli.jar export -workspace architecture/architecture.dsl -format mermaid
java -jar structurizr-cli.jar export -workspace architecture/architecture.dsl -format dot
```

### Using Structurizr Cloud (Online)

1. **Sign up** at https://structurizr.com
2. **Create a workspace**
3. **Import the DSL file** or paste the contents
4. **View and share** diagrams online

## Diagram Views

### 1. System Context

Shows the Trading System in relation to:
- **Trader**: The user of the system
- **Binance Exchange**: External exchange system
- **MongoDB**: Configuration persistence
- **TUI CLI**: External terminal interface

### 2. Container View

Shows the major containers (services) within the Trading System:
- Exchange Engine
- Market Data Engine
- Trading Engine
- Indicator Engine
- Asset Engine
- Live Config Service
- Runner

### 3. Component Views

Detailed component views for each service:
- **Exchange Engine Components**: StateManager, State, Publisher, Reconciler, Driver
- **Market Data Engine Components**: StateManager, State, Publisher, Driver
- **Trading Engine Components**: StateManager, LevelManager, Executor, RiskManager, etc.
- **Indicator Engine Components**: Service, Calculator, Subscriber, Publisher
- **Asset Engine Components**: Service, Cache, PublicClient
- **Live Config Service Components**: Service, Validator, Updaters, Logger
- **Runner Components**: Main, gRPC Server, Service, ShutdownManager

### 4. Deployment View

Shows the production deployment architecture:
- Trading Server (all services)
- MongoDB Server
- Trader Workstation (TUI CLI)
- Binance Exchange

## Color Coding

The diagrams use color coding to distinguish different types of elements:

- **Blue (Internal)**: Internal Trading System components
- **Gray (External)**: External systems (Binance, MongoDB, TUI CLI)
- **Light Blue (StateManager)**: State management services
- **Light Blue (Cache)**: Caching services
- **Dark Blue (Orchestrator)**: Orchestration services

## Relationship Types

- **Broadcast Channel**: Ultra-low latency in-process communication (< 1μs)
- **Direct Call**: Direct function calls within the monolith
- **gRPC**: External gRPC communication
- **HTTPS/WSS**: External HTTP/WebSocket communication
- **MongoDB Protocol**: Database communication

## Customization

You can customize the diagram by editing `architecture/architecture.dsl`:

### Adding New Components

```dsl
newComponent = component "NewComponent" "Description" "Technology"
```

### Adding Relationships

```dsl
componentA -> componentB "Description" "Technology"
```

### Modifying Styles

Edit the `styles` section to change colors, fonts, or line styles.

## Integration with Documentation

The Structurizr diagrams complement the architecture documentation:

- **System Context**: Overview of the entire system
- **Container View**: High-level service architecture
- **Component Views**: Detailed service internals
- **Deployment View**: Production deployment

These diagrams can be embedded in:
- Architecture documentation
- README files
- Presentation slides
- Design documents

## Export Formats

Structurizr supports multiple export formats:

- **PlantUML**: For documentation and wikis
- **Mermaid**: For markdown documentation
- **DOT/Graphviz**: For custom rendering
- **PNG/SVG**: For presentations
- **JSON**: For programmatic access

## Example: Export to PlantUML

```bash
java -jar structurizr-cli.jar export \
  -workspace architecture/architecture.dsl \
  -format plantuml \
  -output diagrams/
```

This will create PlantUML files that can be rendered in documentation or wikis.

## Example: Export to Mermaid

```bash
java -jar structurizr-cli.jar export \
  -workspace architecture/architecture.dsl \
  -format mermaid \
  -output diagrams/
```

This will create Mermaid diagrams that can be embedded in markdown files.

## Tips

1. **Keep DSL Updated**: Update `architecture/architecture.dsl` when the architecture changes
2. **Version Control**: Commit the DSL file to version control
3. **Documentation**: Use exported diagrams in architecture documentation
4. **Sharing**: Share Structurizr Cloud links for collaboration
5. **Automation**: Integrate diagram generation into CI/CD pipelines

## Resources

- **Structurizr Documentation**: https://structurizr.com/help
- **Structurizr DSL Reference**: https://github.com/structurizr/dsl
- **Structurizr Lite**: https://github.com/structurizr/lite
- **Structurizr CLI**: https://github.com/structurizr/cli

## Troubleshooting

### Java Not Found

```bash
# Check Java version
java -version

# Install Java 11+ if needed
# Ubuntu/Debian:
sudo apt-get install openjdk-11-jdk

# macOS:
brew install openjdk@11
```

### DSL Syntax Errors

Structurizr will report syntax errors in the DSL file. Common issues:
- Missing quotes around strings
- Incorrect relationship syntax
- Undefined elements referenced in relationships

### Diagram Not Rendering

- Check that all referenced elements are defined
- Verify relationship syntax is correct
- Ensure tags are properly applied

## Next Steps

1. **View the Diagrams**: Use Structurizr Lite to view the diagrams
2. **Customize**: Modify the DSL to match your specific needs
3. **Export**: Export diagrams for documentation
4. **Share**: Share diagrams with your team
5. **Maintain**: Keep the DSL updated as the architecture evolves

