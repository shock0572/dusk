NAME    := dusk
VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

TARGETS := x86_64-unknown-linux-gnu x86_64-pc-windows-gnu x86_64-apple-darwin
DIST    := dist

.PHONY: all build release clean install uninstall fmt lint test cross dist help

all: build

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

fmt:
	cargo fmt

lint:
	cargo fmt --check
	cargo clippy -- -D warnings

clean:
	cargo clean
	rm -rf $(DIST)

install: release
	cargo install --path .

uninstall:
	cargo uninstall $(NAME)

cross: $(TARGETS)

$(TARGETS):
	cargo build --release --target $@
	@mkdir -p $(DIST)
	@if echo $@ | grep -q windows; then \
		cp target/$@/release/$(NAME).exe $(DIST)/$(NAME)-$(VERSION)-$@.exe; \
	else \
		cp target/$@/release/$(NAME) $(DIST)/$(NAME)-$(VERSION)-$@; \
	fi
	@echo "Built: $(DIST)/$(NAME)-$(VERSION)-$@"

dist: cross
	@echo "All binaries in $(DIST)/"
	@ls -lh $(DIST)/

help:
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  build      Build debug binary (default)"
	@echo "  release    Build optimized release binary"
	@echo "  test       Run tests"
	@echo "  fmt        Format code"
	@echo "  lint       Check formatting and clippy warnings"
	@echo "  clean      Remove build artifacts"
	@echo "  install    Install to cargo bin directory"
	@echo "  uninstall  Remove from cargo bin directory"
	@echo "  cross      Cross-compile for Linux, Windows, macOS"
	@echo "  dist       Cross-compile and collect binaries in dist/"
	@echo "  help       Show this help"
