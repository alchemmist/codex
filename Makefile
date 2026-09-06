SHELL := /bin/zsh
.DEFAULT_GOAL := install-local

CARGO ?= cargo
INSTALL ?= install
CODEX_RS_DIR := $(CURDIR)/codex-rs
CODEX_TARGET_DIR := $(CODEX_RS_DIR)/target
CODEX_BINARY := $(CODEX_TARGET_DIR)/release/codex
BASELINE_BINARY ?= $(CODEX_BINARY)
CODEX_CODE_MODE_HOST_BINARY := $(CODEX_TARGET_DIR)/release/codex-code-mode-host
CODEX_INSTALL_DIR ?= $(HOME)/.local/bin
CODEX_RELEASE_REPOSITORY ?= alchemmist/codex
CODEX_GIT_COMMIT := $(shell git rev-parse --short=8 HEAD 2>/dev/null || printf unknown)
CODEX_GIT_DIRTY := $(shell test -z "$$(git status --porcelain --untracked-files=normal -- . ':(exclude)codex-conversation-*.html' 2>/dev/null)" || printf +dirty)
CODEX_BUILD_COMMIT := $(CODEX_GIT_COMMIT)$(CODEX_GIT_DIRTY)
CODEX_FORK_VERSION := $(shell tr -d '[:space:]' < "$(CURDIR)/FORK_VERSION")

.PHONY: build install-local install-mac install-linux release-patch release-minor release-major

.PHONY: migration-baseline test-migration

migration-baseline:
	python3 scripts/antex-baseline.py $(BASELINE_ARGS)

test-migration:
	python3 -m unittest discover -s scripts -p 'test_antex_*.py'

.PHONY: migration-smoke

migration-smoke:
	python3 scripts/antex-smoke.py --binary "$(BASELINE_BINARY)" $(SMOKE_ARGS)

build:
	CODEX_REPO_ROOT="$(CURDIR)" CARGO_TARGET_DIR="$(CODEX_TARGET_DIR)" STABLE_GIT_COMMIT="$(CODEX_BUILD_COMMIT)" ALCHEMMIST_FORK_VERSION="$(CODEX_FORK_VERSION)" python3 scripts/build-fork-local.py "$(CARGO)"

install-local: build
	$(INSTALL) -d "$(CODEX_INSTALL_DIR)"
	$(INSTALL) -m 755 "$(CODEX_BINARY)" "$(CODEX_INSTALL_DIR)/codex"
	$(INSTALL) -m 755 "$(CODEX_CODE_MODE_HOST_BINARY)" "$(CODEX_INSTALL_DIR)/codex-code-mode-host"
	@/bin/zsh -fc 'rehash'
	@echo "Installed $(CODEX_INSTALL_DIR)/codex"

install-mac:
	CODEX_INSTALL_DIR="$(CODEX_INSTALL_DIR)" CODEX_RELEASE_REPOSITORY="$(CODEX_RELEASE_REPOSITORY)" ./scripts/install-fork-release.sh mac

install-linux:
	CODEX_INSTALL_DIR="$(CODEX_INSTALL_DIR)" CODEX_RELEASE_REPOSITORY="$(CODEX_RELEASE_REPOSITORY)" ./scripts/install-fork-release.sh linux

release-patch:
	./scripts/release-fork.sh patch

release-minor:
	./scripts/release-fork.sh minor

release-major:
	./scripts/release-fork.sh major
