CARGO ?= cargo

.DEFAULT_GOAL := help

.PHONY: help build release check test fmt fmt-check lint ci run clean phpt phpt-lexer

help:
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}'

build: 
	$(CARGO) build --workspace --all-targets

release: 
	$(CARGO) build --release -p phera-cli

check: 
	$(CARGO) check --workspace --all-targets

test: 
	$(CARGO) test --workspace

fmt: 
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all --check

lint:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

ci: fmt-check lint test

run: 
	$(CARGO) run -p phera-cli -- $(ARGS)

clean: 
	$(CARGO) clean

phpt:
	$(CARGO) test -p phera-cli --test phpt -- $(FILTER)

# Set PHP_SRC to a php-src checkout to compare against all of its tests.
phpt-lexer:
	PHERA_LEX_SCAN=$(PHP_SRC) $(CARGO) test -p phera-phpt --test lexer -- $(FILTER)
