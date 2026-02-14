# OpenSwarm - Build and Distribution Makefile
#
# Usage:
#   make build         - Build release binary for current platform
#   make install       - Install binary to /usr/local/bin
#   make test          - Run all tests
#   make clean         - Remove build artifacts
#   make dist          - Create distributable archive
#   make cross-linux   - Cross-compile for Linux x86_64
#   make cross-macos   - Cross-compile for macOS x86_64
#   make cross-all     - Cross-compile for all supported targets

BINARY_NAME := openswarm-connector
VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
CARGO := cargo
INSTALL_DIR := /usr/local/bin
DIST_DIR := dist

# Detect OS and architecture
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

ifeq ($(UNAME_S),Linux)
    OS := linux
endif
ifeq ($(UNAME_S),Darwin)
    OS := macos
endif
ifeq ($(UNAME_M),x86_64)
    ARCH := amd64
endif
ifeq ($(UNAME_M),aarch64)
    ARCH := arm64
endif
ifeq ($(UNAME_M),arm64)
    ARCH := arm64
endif

.PHONY: all build release debug test check clean install uninstall dist \
        cross-linux cross-linux-arm cross-macos cross-all \
        fmt lint doc help

## Default target
all: build

## Build release binary
build:
	$(CARGO) build --release
	@echo ""
	@echo "Binary built: target/release/$(BINARY_NAME)"
	@echo "  Version: $(VERSION)"
	@ls -lh target/release/$(BINARY_NAME)

## Build debug binary
debug:
	$(CARGO) build
	@echo "Debug binary: target/debug/$(BINARY_NAME)"

## Run all tests
test:
	$(CARGO) test --workspace

## Run clippy lint checks
lint:
	$(CARGO) clippy --workspace -- -D warnings

## Format code
fmt:
	$(CARGO) fmt --all

## Check compilation without building
check:
	$(CARGO) check --workspace

## Generate documentation
doc:
	$(CARGO) doc --workspace --no-deps --open

## Clean build artifacts
clean:
	$(CARGO) clean
	rm -rf $(DIST_DIR)

## Install binary to system path
install: build
	@echo "Installing $(BINARY_NAME) to $(INSTALL_DIR)..."
	install -d $(INSTALL_DIR)
	install -m 755 target/release/$(BINARY_NAME) $(INSTALL_DIR)/$(BINARY_NAME)
	@echo "Installed: $(INSTALL_DIR)/$(BINARY_NAME)"
	@echo ""
	@echo "Run: $(BINARY_NAME) --help"

## Uninstall binary from system path
uninstall:
	rm -f $(INSTALL_DIR)/$(BINARY_NAME)
	@echo "Removed: $(INSTALL_DIR)/$(BINARY_NAME)"

## Create distributable archive for current platform
dist: build
	@mkdir -p $(DIST_DIR)
	@echo "Creating distribution archive..."
	tar czf $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-$(OS)-$(ARCH).tar.gz \
		-C target/release $(BINARY_NAME) \
		-C ../../ README.md QUICKSTART.md docs/SKILL.md docs/HEARTBEAT.md docs/MESSAGING.md
	@echo "Archive: $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-$(OS)-$(ARCH).tar.gz"
	@ls -lh $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-$(OS)-$(ARCH).tar.gz

## Cross-compile for Linux x86_64
cross-linux:
	$(CARGO) build --release --target x86_64-unknown-linux-gnu
	@mkdir -p $(DIST_DIR)
	tar czf $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-linux-amd64.tar.gz \
		-C target/x86_64-unknown-linux-gnu/release $(BINARY_NAME) \
		-C ../../../ README.md QUICKSTART.md docs/SKILL.md docs/HEARTBEAT.md docs/MESSAGING.md
	@echo "Built: $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-linux-amd64.tar.gz"

## Cross-compile for Linux ARM64
cross-linux-arm:
	$(CARGO) build --release --target aarch64-unknown-linux-gnu
	@mkdir -p $(DIST_DIR)
	tar czf $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-linux-arm64.tar.gz \
		-C target/aarch64-unknown-linux-gnu/release $(BINARY_NAME) \
		-C ../../../ README.md QUICKSTART.md docs/SKILL.md docs/HEARTBEAT.md docs/MESSAGING.md
	@echo "Built: $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-linux-arm64.tar.gz"

## Cross-compile for macOS x86_64
cross-macos:
	$(CARGO) build --release --target x86_64-apple-darwin
	@mkdir -p $(DIST_DIR)
	tar czf $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-macos-amd64.tar.gz \
		-C target/x86_64-apple-darwin/release $(BINARY_NAME) \
		-C ../../../ README.md QUICKSTART.md docs/SKILL.md docs/HEARTBEAT.md docs/MESSAGING.md
	@echo "Built: $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-macos-amd64.tar.gz"

## Cross-compile for macOS ARM64
cross-macos-arm:
	$(CARGO) build --release --target aarch64-apple-darwin
	@mkdir -p $(DIST_DIR)
	tar czf $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-macos-arm64.tar.gz \
		-C target/aarch64-apple-darwin/release $(BINARY_NAME) \
		-C ../../../ README.md QUICKSTART.md docs/SKILL.md docs/HEARTBEAT.md docs/MESSAGING.md
	@echo "Built: $(DIST_DIR)/$(BINARY_NAME)-$(VERSION)-macos-arm64.tar.gz"

## Cross-compile for all supported targets
cross-all: cross-linux cross-linux-arm cross-macos cross-macos-arm
	@echo ""
	@echo "All cross-compilation targets built:"
	@ls -lh $(DIST_DIR)/

## Show help
help:
	@echo "OpenSwarm Build System"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Build Targets:"
	@echo "  build          Build release binary for current platform"
	@echo "  debug          Build debug binary"
	@echo "  check          Check compilation without building"
	@echo "  test           Run all tests"
	@echo "  lint           Run clippy lints"
	@echo "  fmt            Format all code"
	@echo "  doc            Generate and open documentation"
	@echo "  clean          Remove build artifacts"
	@echo ""
	@echo "Install Targets:"
	@echo "  install        Install binary to $(INSTALL_DIR)"
	@echo "  uninstall      Remove installed binary"
	@echo ""
	@echo "Distribution Targets:"
	@echo "  dist           Create archive for current platform"
	@echo "  cross-linux    Cross-compile for Linux x86_64"
	@echo "  cross-linux-arm Cross-compile for Linux ARM64"
	@echo "  cross-macos    Cross-compile for macOS x86_64"
	@echo "  cross-macos-arm Cross-compile for macOS ARM64"
	@echo "  cross-all      Cross-compile for all targets"
	@echo ""
	@echo "Current platform: $(OS)-$(ARCH)"
	@echo "Version: $(VERSION)"
