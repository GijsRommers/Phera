CARGO ?= cargo

.DEFAULT_GOAL := help

.PHONY: help build release check test fmt fmt-check lint ci ci-prerequisites ci-build test-php86 safe-rust run clean phpt phpt-sweep phpt-lexer

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
	$(CARGO) test --workspace --no-fail-fast

fmt: 
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all --check

lint:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

# Keep prerequisite verification ahead of tests even with make -j.
.NOTPARALLEL: ci
ci: fmt-check lint safe-rust ci-prerequisites test test-php86

safe-rust:
	python3 .github/scripts/check-unsafe.py

ci-prerequisites:
	@case "$${PHERA_TEST_PHP:-8.5}" in \
	  8.5) "$${PHERA_PHP85:-$${PHERA_PHP:-php}}" -n .github/scripts/check-prerequisites.php 8.5 ;; \
	  8.6) "$${PHERA_PHP86:?Set PHERA_PHP86 to the PHP 8.6 oracle}" \
	    -n .github/scripts/check-prerequisites.php 8.6 ;; \
	  *) echo 'PHERA_TEST_PHP must be 8.5 or 8.6' >&2; exit 2 ;; \
	esac

test-php86:
	@if [ -n "$${PHERA_PHP86:-}" ]; then \
	  PHERA_TEST_PHP=8.6 $(MAKE) ci-prerequisites && \
	  PHERA_TEST_PHP=8.6 $(CARGO) test --workspace --no-fail-fast; \
	else \
	  echo 'Skipping PHP 8.6 tests. Set PHERA_PHP86 to the PHP 8.6 oracle to enable them.'; \
	fi

ci-build:
	$(CARGO) build --release -p phera-cli

run: 
	$(CARGO) run -p phera-cli -- $(ARGS)

clean: 
	$(CARGO) clean

phpt:
	$(CARGO) test -p phera-cli --test phpt -- $(FILTER)

# Set PHP_SRC to a php-src checkout: adds every test of it that passes on real
# PHP and on Phera to tests/upstream/ (see tests/phpt/sweep.sh).
phpt-sweep:
	tests/phpt/sweep.sh $(PHP_SRC)

# Set PHP_SRC to a php-src checkout to compare against all of its tests.
phpt-lexer:
	PHERA_LEX_SCAN=$(PHP_SRC) $(CARGO) test -p phera-phpt --test lexer -- $(FILTER)
