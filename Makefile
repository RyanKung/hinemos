SHELL := /bin/bash

REPO ?= .
HOST ?= agentopia
REMOTE_DIR ?= /opt/hinemos
SSH ?= ssh -o ControlMaster=no -o ControlPath=none
RSYNC ?= rsync -az --delete -e '$(SSH)'
HOST_BUILD_DIR ?= $(REPO)/.host-build
HOST_BINARY ?= $(HOST_BUILD_DIR)/hinemos
LANDING_DIST ?= $(REPO)/web/landing/dist
HINEMOS_RELEASE_TARGET ?= x86_64-unknown-linux-gnu

.PHONY: help fmt fmt-fix check test build-host build-landing sync-source sync-binary sync-landing install-host restart-host deploy-host status-host clean-host test-landing-agent-matrix

help:
	@printf '%s\n' \
		'Available targets:' \
		'  make fmt            - Check formatting' \
		'  make fmt-fix        - Rewrite formatting' \
		'  make check          - Run cargo check' \
		'  make test           - Run workspace lib tests' \
		'  make build-host     - Build a Linux host binary locally' \
		'  make build-landing  - Build the landing page dist bundle' \
		'  make sync-source    - Sync repo sources to the remote host' \
		'  make sync-binary    - Upload the built hinemos binary to the host' \
		'  make sync-landing   - Upload the landing dist bundle to the host' \
		'  make install-host   - Install the remote systemd services' \
		'  make restart-host   - Restart the remote Hinemos services' \
		'  make deploy-host    - Build, sync, install, and restart' \
		'  make test-landing-agent-matrix - Run the landing page agent matrix' \
		'  make status-host    - Show remote service status' \
		'  make clean-host     - Remove local host-build/cache artifacts'

fmt:
	cargo fmt --check

fmt-fix:
	cargo fmt

check:
	cargo check --workspace

test:
	cargo test --workspace --lib

build-host:
	HINEMOS_RELEASE_TARGET=$(HINEMOS_RELEASE_TARGET) scripts/host-release-build.sh $(REPO)

build-landing:
	cd $(REPO)/web/landing && env -u NO_COLOR trunk build --release

sync-source:
	$(RSYNC) \
		--exclude .cargo-home \
		--exclude .cache \
		--exclude .host-build \
		--exclude .playwright-cli \
		--exclude target \
		--exclude web/landing/dist \
		--exclude .env \
		--exclude .env.local \
		--exclude .env.test \
		--exclude .env.production \
		--exclude .env.prod \
		--exclude .hinemos \
		--exclude .xagora \
		--exclude .claude \
		--exclude .DS_Store \
		$(REPO)/ $(HOST):$(REMOTE_DIR)/

sync-binary:
	$(RSYNC) $(HOST_BINARY) $(HOST):$(REMOTE_DIR)/.host-build/hinemos

sync-landing:
	$(RSYNC) $(LANDING_DIST)/ $(HOST):$(REMOTE_DIR)/web/landing/dist/

install-host:
	$(SSH) $(HOST) 'cd $(REMOTE_DIR) && sudo scripts/install-host-service.sh $(REMOTE_DIR) && sudo scripts/install-host-http-service.sh $(REMOTE_DIR)'

restart-host:
	$(SSH) $(HOST) 'sudo systemctl restart hinemos.service hinemos-http.service hinemos-mail.service hinemos-rooms.service'

deploy-host: build-host build-landing sync-source sync-binary sync-landing install-host restart-host

test-landing-agent-matrix:
	scripts/landing_agent_matrix.sh

status-host:
	$(SSH) $(HOST) 'systemctl is-active hinemos.service hinemos-http.service hinemos-mail.service hinemos-rooms.service && cd $(REMOTE_DIR) && git rev-parse --short HEAD'

clean-host:
	rm -rf $(REPO)/.cargo-home $(REPO)/.cache $(REPO)/.host-build
