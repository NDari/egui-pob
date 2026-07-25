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

# Pre-pin review of an upstream upgrade: changelog + scale between two tags.
# Read-only; after moving the pin, run `cargo test --test ports_sync`.
upgrade-review old new:
    @echo "== commits {{old}}..{{new}} =="
    @git -C upstream log --oneline {{old}}..{{new}} | wc -l
    @echo "== code diff (excluding generated data) =="
    @git -C upstream diff --stat {{old}}..{{new}} -- src/Classes/ src/Modules/ | tail -1
    @echo "== changelog =="
    @git -C upstream show {{new}}:CHANGELOG.md | awk -v tag="{{old}}" '/^## \[/{if (index($0, tag)) exit} {print}'
    @echo "== next: diff the ports.toml anchors, move the pin, then: cargo test --test ports_sync && just test =="
