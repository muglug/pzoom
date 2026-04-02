# pzoom

A fast PHP static analyzer written in Rust.

> [!NOTE]
> This is not something I ever intend to support. Also, it is vibe-coded with no great care taken as to its overall quality. Caveat emptor.

## Overview

pzoom is a static analysis tool for PHP that detects type errors, unused code, and other issues in your codebase. It's designed for speed and Psalm compatibility.

### Features

- Fast parallel analysis using Rust
- Psalm-compatible configuration (reads `psalm.xml`)
- Type inference and checking
- Unused code detection
- Taint analysis for security vulnerabilities

## Installation

```bash
cargo install --path crates/pzoom-cli
```

Or build from source:

```bash
cargo build --release
```

## Usage

### Basic Analysis

```bash
# Analyze current directory
pzoom

# Analyze specific paths
pzoom analyze src/ lib/

# Analyze with custom config
pzoom --config psalm.xml src/
```

### Commands

| Command | Description |
|---------|-------------|
| `pzoom` | Analyze current directory (default) |
| `pzoom analyze [paths]` | Analyze specified paths |
| `pzoom info` | Show version information |
| `pzoom clear-cache` | Clear the analysis cache |

### Options

| Option | Description |
|--------|-------------|
| `-c, --config <path>` | Path to configuration file |
| `-f, --format <format>` | Output format: text, json, checkstyle |
| `-t, --threads <n>` | Number of threads to use |
| `--errors-only` | Show only errors (no warnings) |

## Configuration

pzoom reads Psalm's XML configuration format. It looks for `psalm.xml`, `psalm.xml.dist`, or `psalm.dist.xml` in your project root.

```xml
<?xml version="1.0"?>
<psalm
    errorLevel="2"
    phpVersion="8.2"
    findUnusedCode="true"
    runTaintAnalysis="true"
>
    <projectFiles>
        <directory name="src" />
        <ignoreFiles>
            <directory name="vendor" />
        </ignoreFiles>
    </projectFiles>

    <issueHandlers>
        <MixedAssignment errorLevel="suppress" />
    </issueHandlers>

    <stubs>
        <file name="stubs/custom.php" />
    </stubs>

    <forbiddenFunctions>
        <function name="var_dump" />
        <function name="print_r" />
    </forbiddenFunctions>
</psalm>
```

See the [Psalm configuration documentation](https://psalm.dev/docs/running_psalm/configuration/) for more details on available options.

## Architecture

pzoom uses a three-phase analysis pipeline:

1. **Scan** - Parse PHP files and collect symbols (classes, functions, etc.)
2. **Populate** - Resolve inheritance hierarchies and build complete type information
3. **Analyze** - Perform type checking and issue detection

### Crates

| Crate | Description |
|-------|-------------|
| `pzoom-cli` | Command-line interface |
| `pzoom-orchestrator` | Pipeline coordination |
| `pzoom-analyzer` | Core analysis engine |
| `pzoom-code-info` | Type system (TUnion, TAtomic) |
| `pzoom-str` | String interning |
| `pzoom-syntax` | Syntax utilities |
| `pzoom-test-runner` | Inference test runner |

## Development

### Building

```bash
cargo build
```

### Testing

```bash
# Run unit tests
cargo test

# Run inference tests
cargo run -p pzoom-test-runner

# Run specific test category
cargo run -p pzoom-test-runner -- tests/inference/Assignment

# Update expected output files
cargo run -p pzoom-test-runner -- --update
```

### Running

```bash
cargo run -- analyze path/to/php/project
```

## License

MIT
