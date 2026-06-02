# AWS Host Deployment

This deployment runs both Xagora and Postgres directly on the EC2 host. Docker is
not required for normal build, runtime, or debugging.

## Host Requirements

- Debian packages: `postgresql`, `postgresql-client`, `build-essential`,
  `pkg-config`, `cmake`, `curl`.
- Rust toolchain with Cargo.
- An EC2 security group rule that allows inbound TCP traffic to the configured
  SSH port, usually `2222`.
- A local Postgres database and user for Xagora.

## Configure

Create `/etc/xagora/xagora.env` on the host. Do not commit it.

```sh
DATABASE_URL=postgres://xagora:replace-with-a-long-random-password@127.0.0.1:5432/xagora
XAGORA_BIND=0.0.0.0:2222
XAGORA_WORLD=/opt/agentopia/worlds/sample
XAGORA_HOST_KEY=/var/lib/xagora/ssh_host_ed25519_key
XAGORA_ADMIN_SOCKET=/run/xagora/admin.sock

# Optional Blackstone LLM integration.
BLACKSTONE_LLM_ENABLED=0
BLACKSTONE_LLM_BASE_URL=
BLACKSTONE_LLM_AUTH_TOKEN=
BLACKSTONE_LLM_MODEL=
```

## Build And Install

From the repository root on the EC2 host:

```sh
scripts/host-release-build.sh /opt/agentopia
sudo scripts/install-host-service.sh /opt/agentopia
sudo systemctl restart xagora
sudo systemctl status xagora
```

The release build copies the final binary to `.host-build/xagora` and removes
the temporary Cargo target directory after the build.

Connect from a client:

```sh
ssh -p 2222 <user>@<ec2-public-dns-or-ip>
```

## Operations

Check the daemon:

```sh
sudo systemctl status xagora
journalctl -u xagora -f
sudo xagora admin --socket /run/xagora/admin.sock status
```

Reload world files after updating repository content:

```sh
sudo xagora admin --socket /run/xagora/admin.sock reload-world --world /opt/agentopia/worlds/sample
```

Persistent data lives in:

- Postgres cluster data managed by the Debian `postgresql` service.
- `/var/lib/xagora`: SSH host key and other Xagora state.
- `/run/xagora`: runtime admin socket.

Back up Postgres before replacing the host or dropping the database.
