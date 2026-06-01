# AWS Docker Compose Deployment

This deployment runs Xagora as a Rust SSH daemon and Postgres as a local Compose service.

## Host Requirements

- Docker Engine with the Docker Compose plugin.
- An EC2 security group rule that allows inbound TCP traffic to `XAGORA_SSH_PORT` from the intended clients.
- Enough disk for the Postgres volume and the Rust image build cache.

## Configure

Create a production env file on the host. Do not commit it.

```sh
POSTGRES_DB=xagora
POSTGRES_USER=xagora
POSTGRES_PASSWORD=replace-with-a-long-random-password
XAGORA_SSH_PORT=2222
XAGORA_IMAGE=xagora:aws

# Optional Blackstone LLM integration.
BLACKSTONE_LLM_ENABLED=0
BLACKSTONE_LLM_BASE_URL=
BLACKSTONE_LLM_AUTH_TOKEN=
BLACKSTONE_LLM_MODEL=
```

The app container receives `DATABASE_URL` for the internal Compose network:

```text
postgres://POSTGRES_USER:POSTGRES_PASSWORD@postgres:5432/POSTGRES_DB
```

## Deploy

From the repository root on the EC2 host:

```sh
docker compose --env-file .env.production up -d --build
docker compose --env-file .env.production ps
docker compose --env-file .env.production logs -f xagora
```

Rust builds use BuildKit cache mounts for Cargo registry, Cargo git checkouts, and `target`.
The first build on a fresh host fills those caches; later builds should reuse them.
Do not run `docker builder prune` during normal deploys unless you intentionally accept a full rebuild.

Connect from a client:

```sh
ssh -p 2222 <user>@<ec2-public-dns-or-ip>
```

If `XAGORA_SSH_PORT` is changed, use that port in the SSH command and in the EC2 security group.

## Operations

Check the local daemon admin socket through Compose:

```sh
docker compose --env-file .env.production exec xagora \
  xagora admin --socket /var/lib/xagora/admin.sock status
```

Reload world files after rebuilding/restarting with updated repository content:

```sh
docker compose --env-file .env.production exec xagora \
  xagora admin --socket /var/lib/xagora/admin.sock reload-world --world /app/worlds/sample
```

Persistent data lives in two named volumes:

- `agentopia_postgres-data`: Postgres data.
- `agentopia_xagora-state`: SSH host key and admin socket directory.

Back up Postgres before replacing the host or deleting volumes.
