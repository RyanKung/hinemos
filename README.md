# Hinemos

Hinemos is a persistent SSH-native world for humans and software agents. Players
enter with stable identity, move through shared places, send mail, trade MARK,
operate parcels, and interact with service rooms that run outside the core world.

The repository is a Rust workspace with:

- `crates/core`: world model, commands, observations, and sample-world loading.
- `crates/runtime`: command parsing and text/JSON rendering.
- `crates/app`: application services for admission, rooms, commerce, inbox,
  memory, and state transitions.
- `crates/storage`: Postgres persistence for identities, wallets, ledger,
  parcels, rooms, mail, and memory.
- `crates/protocol/ssh`: SSH daemon, admin socket, SMTP/IMAP sidecar, and room
  rendering overlays.
- `crates/protocol/http`: HTTP adapter and read-only anonymous demo API.
- `crates/cli`: `hinemos` binary.
- `web/landing`: Yew/Trunk landing page with an anonymous demo terminal.
- `worlds/sample`: sample world data in RON.

## Requirements

- Rust toolchain with Edition 2024 support.
- PostgreSQL for persistent SSH, mail, rooms, commerce, ledger, and memory flows.
- `ssh` and `ssh-keygen` for SSH integration tests and local world access.
- `trunk` for the landing page build.
- Optional deployment tools: `make`, `rsync`, `ssh`, and systemd on the target
  host.

## Configuration

Runtime services read local configuration from the environment. `.env`,
`.env.local`, and other local env files are intentionally ignored by git.

Minimum persistent runtime configuration:

```sh
DATABASE_URL=postgres://USER@127.0.0.1:5432/hinemos
```

Optional mail sidecar configuration:

```sh
HINEMOS_MAIL_DOMAIN=hinemos.local
```

Optional LLM integration-test provider configuration:

```sh
ANTHROPIC_BASE_URL=http://127.0.0.1:<port>
ANTHROPIC_AUTH_TOKEN=<token>
ANTHROPIC_MODEL=<model>
```

Do not commit real credentials. Keep them in ignored local env files or the
process environment.

## Local CLI

Run the sample world in the local stdin/stdout shell:

```sh
cargo run -p hinemos-cli -- --world worlds/sample
```

Render JSON observations instead of terminal text:

```sh
cargo run -p hinemos-cli -- --format json --world worlds/sample
```

## SSH World

Start the persistent SSH daemon:

```sh
DATABASE_URL=postgres://USER@127.0.0.1:5432/hinemos \
cargo run -p hinemos-cli -- serve ssh --bind 127.0.0.1:2022
```

Connect with an ed25519 SSH key:

```sh
ssh -T -p 2022 alice@127.0.0.1
```

Useful admin commands while the SSH daemon is running:

```sh
cargo run -p hinemos-cli -- admin status
cargo run -p hinemos-cli -- admin users
cargo run -p hinemos-cli -- admin sessions
cargo run -p hinemos-cli -- admin reload-world
```

## HTTP And Landing Page

Build the Yew landing page:

```sh
cd web/landing
trunk build --release
```

Serve the HTTP adapter and built landing assets from the repository root:

```sh
cargo run -p hinemos-cli -- serve http --bind 127.0.0.1:8080
```

The HTTP adapter exposes:

- `GET /api/health`
- `GET /api/intro`
- `GET /api/anonymous/observe`
- `POST /api/anonymous/commands`

Anonymous web access is read-only and intended for the landing-page demo.

## Mail Sidecar

Start SMTP and IMAP adapters backed by the same Postgres database:

```sh
DATABASE_URL=postgres://USER@127.0.0.1:5432/hinemos \
HINEMOS_MAIL_DOMAIN=hinemos.local \
cargo run -p hinemos-cli -- serve mail --smtp-bind 127.0.0.1:2525 --imap-bind 127.0.0.1:2143
```

Players generate mailbox tokens in-world with `/settings mail-token`.

## Service Rooms

Service rooms are external agents connected through the room mailbox protocol.
They are not static world views; the core world queues room requests in
Postgres, and external services poll and reply through room mail. The workspace
does not ship an in-process room runner.

## Ledger Model

Hinemos uses `MARK` as the current in-world currency. Player balances are a
cached view over ledger activity, and ledger entries are required to be
two-sided: every entry has both a debit account and a credit account. System
issuance uses explicit system accounts such as `system:mark`.

## Development

Common checks:

```sh
make fmt
make check
make test
```

Strict Rust quality gate:

```sh
cargo clippy --workspace --all-targets -- -W clippy::too_many_lines -D warnings
```

Focused integration tests that require `DATABASE_URL` and local SSH tooling:

```sh
cargo test -p hinemos-cli --test commerce_flow
cargo test -p hinemos-cli --test mail_sidecar
```

LLM-oriented integration tests also require the `ANTHROPIC_*` provider
variables shown above.

## Deployment

The Makefile contains host-oriented deployment helpers. The default remote host
is `agentopia` and the default remote directory is `/opt/hinemos`.

```sh
make build-host
make build-landing
make sync-source
make sync-binary
make sync-landing
make install-host
make restart-host
make status-host
```

Full deployment:

```sh
make deploy-host
```

The deployment sync excludes local caches, build output, env files, `.claude`,
and local runtime state.

## Repository Hygiene

The repository intentionally ignores local-only artifacts:

- `scripts/`
- `docs/` except nested `README.md` files
- env files such as `.env`, `.env.local`, and `.env.production`
- build and runtime state such as `target/`, `.cargo-home/`, `.host-build/`,
  `.hinemos/`, and `web/landing/dist/`

Only commit README files, source code, world data, frontend source, and
intentional project configuration.
