.PHONY: all build test validate validate-inner purge clean install help upstream bench

# Default target
all: build

# Build the project in debug mode
build:
	cargo build

# Build release version
release:
	cargo build --release

# Run all tests
test:
	cargo test

upstream: beads/bd-upstream
beads/bd-upstream:
	cd beads && go build -o bd-upstream ./cmd/bd

# Validation target - runs all checks before commit with timing
validate: purge
	@time $(MAKE) validate-inner

# Internal validation target that does the actual work
validate-inner: test
	@echo "Running cargo fmt check..."
	cargo fmt -- --check
	@echo "Running clippy..."
	cargo clippy -- -D warnings
	@echo "All validation checks passed!"

# Purge upstream bd processes and database files before validation
purge:
	@echo "Purging upstream bd artifacts..."
	@./purge-bd-upstream.sh

# Format code
fmt:
	cargo fmt

# Run benchmarks
bench: release
	@echo "Running benchmarks with release build..."
	@./benches/benchmark.sh

# Clean build artifacts
clean:
	cargo clean
	rm -rf scratch/*/beads.lock
	(cd scratch && git clean -fxd)

# Install binary to ~/.local/bin
install: release
	mkdir -p ~/.local/bin
	cp target/release/bd ~/.local/bin/

# Show help
help:
	@echo "Minibeads Makefile targets:"
	@echo "  make build     - Build debug binary"
	@echo "  make release   - Build release binary"
	@echo "  make test      - Run unit tests"
	@echo "  make validate  - Run all validation checks (test, fmt, clippy)"
	@echo "  make fmt       - Format code with rustfmt"
	@echo "  make bench     - Run performance benchmarks"
	@echo "  make clean     - Clean build artifacts"
	@echo "  make install   - Install release binary to ~/.local/bin"
	@echo "  make help      - Show this help message"
