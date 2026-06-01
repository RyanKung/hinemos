# syntax=docker/dockerfile:1

FROM rust:1-bookworm AS builder

WORKDIR /app
ENV CARGO_INCREMENTAL=1

RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates/admin-protocol/Cargo.toml crates/admin-protocol/Cargo.toml
COPY crates/blackstone/Cargo.toml crates/blackstone/Cargo.toml
COPY crates/cli/Cargo.toml crates/cli/Cargo.toml
COPY crates/core/Cargo.toml crates/core/Cargo.toml
COPY crates/protocol/ssh/Cargo.toml crates/protocol/ssh/Cargo.toml
COPY crates/runtime/Cargo.toml crates/runtime/Cargo.toml
COPY crates/storage/Cargo.toml crates/storage/Cargo.toml
RUN mkdir -p \
        crates/admin-protocol/src \
        crates/blackstone/src \
        crates/cli/src \
        crates/core/src \
        crates/protocol/ssh/src \
        crates/runtime/src \
        crates/storage/src \
    && touch \
        crates/admin-protocol/src/lib.rs \
        crates/blackstone/src/lib.rs \
        crates/core/src/lib.rs \
        crates/protocol/ssh/src/lib.rs \
        crates/runtime/src/lib.rs \
        crates/storage/src/lib.rs \
    && printf 'fn main() {}\\n' > crates/cli/src/main.rs
RUN --mount=type=cache,id=agentopia-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=agentopia-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    cargo fetch --locked

COPY crates ./crates
COPY worlds ./worlds

RUN --mount=type=cache,id=agentopia-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=agentopia-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=agentopia-target,target=/app/target,sharing=locked \
    cargo build --release --bin xagora \
    && cp /app/target/release/xagora /usr/local/bin/xagora-build

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --home-dir /var/lib/xagora --shell /usr/sbin/nologin xagora

WORKDIR /app

COPY --from=builder /usr/local/bin/xagora-build /usr/local/bin/xagora
COPY worlds ./worlds

RUN mkdir -p /var/lib/xagora \
    && chown -R xagora:xagora /var/lib/xagora /app

USER xagora

EXPOSE 2222

CMD ["xagora", "serve", "ssh", "--bind", "0.0.0.0:2222", "--world", "/app/worlds/sample", "--host-key", "/var/lib/xagora/ssh_host_ed25519_key", "--admin-socket", "/var/lib/xagora/admin.sock"]
