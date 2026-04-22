# Default recipe: list available commands
default:
    @just --list

# Release build + run
run:
    RUST_LOG=info cargo run --release

# Watch for changes and re-run (release)
watch:
    RUST_LOG=info cargo watch -x 'run --release'

# Debug build + run
debug:
    RUST_LOG=debug cargo run

# Release build
build:
    cargo build --release

# Debug build
build-debug:
    cargo build

# Run all tests
test:
    cargo test

# Run clippy lints
clippy:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Type-check without building
check:
    cargo check

# Update upstream submodule to latest tag
sync:
    cd upstream && git fetch origin && git checkout $(git describe --tags --abbrev=0 origin/master)
    @echo "Updated upstream to $(cd upstream && git describe --tags)"
    @echo "Run 'just build' to verify it compiles"
