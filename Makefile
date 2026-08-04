SHELL := /bin/zsh
.DEFAULT_GOAL := install

CARGO ?= cargo
INSTALL ?= install
CODEX_RS_DIR := $(CURDIR)/codex-rs
CODEX_TARGET_DIR := $(CODEX_RS_DIR)/target
CODEX_BINARY := $(CODEX_TARGET_DIR)/release/codex
CODEX_INSTALL_DIR ?= $(HOME)/.local/bin

.PHONY: build install

build:
	cd "$(CODEX_RS_DIR)" && CARGO_TARGET_DIR="$(CODEX_TARGET_DIR)" $(CARGO) build --release --bin codex

install: build
	$(INSTALL) -d "$(CODEX_INSTALL_DIR)"
	$(INSTALL) -m 755 "$(CODEX_BINARY)" "$(CODEX_INSTALL_DIR)/codex"
	@/bin/zsh -fc 'rehash'
	@echo "Installed $(CODEX_INSTALL_DIR)/codex"
