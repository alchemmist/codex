SHELL := /bin/sh
.DEFAULT_GOAL := install-local

CARGO ?= cargo
INSTALL ?= install
ANTEX_RS_DIR := $(CURDIR)/antex-rs
ANTEX_TARGET_DIR := $(ANTEX_RS_DIR)/target
ANTEX_BINARY := $(ANTEX_TARGET_DIR)/release/antex
ANTEX_INSTALL_DIR ?= $(HOME)/.local/bin

.PHONY: build install-local install-mac install-linux release-patch release-minor release-major migration-baseline

build:
	$(CARGO) build --locked --release --manifest-path "$(ANTEX_RS_DIR)/Cargo.toml" --bin antex

install-local: build
	$(INSTALL) -d "$(ANTEX_INSTALL_DIR)"
	$(INSTALL) -m 755 "$(ANTEX_BINARY)" "$(ANTEX_INSTALL_DIR)/antex"
	@echo "Installed $(ANTEX_INSTALL_DIR)/antex"

install-mac:
	./scripts/install-antex-release.sh mac

install-linux:
	./scripts/install-antex-release.sh linux

release-patch:
	./scripts/release-antex.sh patch

release-minor:
	./scripts/release-antex.sh minor

release-major:
	./scripts/release-antex.sh major

migration-baseline:
	python3 scripts/antex-baseline.py $(BASELINE_ARGS)
