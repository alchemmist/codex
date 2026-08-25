SHELL := /bin/zsh
.DEFAULT_GOAL := install

CARGO ?= cargo
INSTALL ?= install
CODEX_RS_DIR := $(CURDIR)/codex-rs
CODEX_TARGET_DIR := $(CODEX_RS_DIR)/target
CODEX_BINARY := $(CODEX_TARGET_DIR)/release/codex
CODEX_INSTALL_DIR ?= $(HOME)/.local/bin
CODEX_GIT_COMMIT := $(shell git rev-parse --short=8 HEAD 2>/dev/null || printf unknown)
CODEX_GIT_DIRTY := $(shell test -z "$$(git status --porcelain --untracked-files=normal -- . ':(exclude)codex-conversation-*.html' 2>/dev/null)" || printf +dirty)
CODEX_BUILD_COMMIT := $(CODEX_GIT_COMMIT)$(CODEX_GIT_DIRTY)

.PHONY: build install

build:
	cd "$(CODEX_RS_DIR)" && CARGO_TARGET_DIR="$(CODEX_TARGET_DIR)" STABLE_GIT_COMMIT="$(CODEX_BUILD_COMMIT)" $(CARGO) build --release --bin codex

install: build
	$(INSTALL) -d "$(CODEX_INSTALL_DIR)"
	$(INSTALL) -m 755 "$(CODEX_BINARY)" "$(CODEX_INSTALL_DIR)/codex"
	@/bin/zsh -fc 'rehash'
	@echo "Installed $(CODEX_INSTALL_DIR)/codex"
