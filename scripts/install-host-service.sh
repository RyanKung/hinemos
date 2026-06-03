#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage: install-host-service.sh [repo]

Install the host-built hinemos binary and systemd service.
Expected binary: <repo>/.host-build/hinemos
Expected env file: /etc/hinemos/hinemos.env
USAGE
  exit 0
fi

repo="${1:-/opt/hinemos}"
binary="$repo/.host-build/hinemos"
service_file="/etc/systemd/system/hinemos.service"
env_dir="/etc/hinemos"
env_file="$env_dir/hinemos.env"

if [[ $EUID -ne 0 ]]; then
  echo "install-host-service.sh must run as root" >&2
  exit 1
fi

if [[ ! -x "$binary" ]]; then
  echo "missing executable build artifact: $binary" >&2
  exit 1
fi

if ! id -u hinemos >/dev/null 2>&1; then
  useradd --system --create-home --home-dir /var/lib/hinemos --shell /usr/sbin/nologin hinemos
fi

install -d -m 0755 "$env_dir"
install -d -o hinemos -g hinemos -m 0750 /var/lib/hinemos
install -m 0755 "$binary" /usr/local/bin/hinemos

if [[ ! -f "$env_file" ]]; then
  cat >"$env_file" <<'ENV'
DATABASE_URL=postgres://hinemos:change-me@127.0.0.1:5432/hinemos
HINEMOS_BIND=127.0.0.1:2022
HINEMOS_WORLD=/opt/hinemos/worlds/sample
HINEMOS_HOST_KEY=/var/lib/hinemos/ssh_host_ed25519_key
HINEMOS_ADMIN_SOCKET=/run/hinemos/admin.sock
HINEMOS_MAIL_DOMAIN=hinemos.local
BLACKSTONE_LLM_ENABLED=0
BLACKSTONE_LLM_BASE_URL=
BLACKSTONE_LLM_AUTH_TOKEN=
BLACKSTONE_LLM_MODEL=
ENV
  chmod 0600 "$env_file"
  chown root:root "$env_file"
fi

cat >"$service_file" <<'UNIT'
[Unit]
Description=Hinemos SSH daemon
After=network-online.target postgresql.service
Wants=network-online.target
Requires=postgresql.service

[Service]
Type=simple
User=hinemos
Group=hinemos
WorkingDirectory=/opt/hinemos
EnvironmentFile=/etc/hinemos/hinemos.env
RuntimeDirectory=hinemos
RuntimeDirectoryMode=0755
StateDirectory=hinemos
StateDirectoryMode=0750
ExecStart=/usr/local/bin/hinemos serve ssh --bind ${HINEMOS_BIND} --world ${HINEMOS_WORLD} --host-key ${HINEMOS_HOST_KEY} --admin-socket ${HINEMOS_ADMIN_SOCKET}
Restart=always
RestartSec=3
NoNewPrivileges=true
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
PrivateTmp=true

[Install]
WantedBy=multi-user.target
UNIT

systemctl daemon-reload
systemctl enable hinemos.service

printf 'Installed hinemos service with binary: /usr/local/bin/hinemos\n'
