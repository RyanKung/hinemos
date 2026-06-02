#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage: install-host-service.sh [repo]

Install the host-built xagora binary and systemd service.
Expected binary: <repo>/.host-build/xagora
Expected env file: /etc/xagora/xagora.env
USAGE
  exit 0
fi

repo="${1:-/opt/agentopia}"
binary="$repo/.host-build/xagora"
service_file="/etc/systemd/system/xagora.service"
env_dir="/etc/xagora"
env_file="$env_dir/xagora.env"

if [[ $EUID -ne 0 ]]; then
  echo "install-host-service.sh must run as root" >&2
  exit 1
fi

if [[ ! -x "$binary" ]]; then
  echo "missing executable build artifact: $binary" >&2
  exit 1
fi

if ! id -u xagora >/dev/null 2>&1; then
  useradd --system --create-home --home-dir /var/lib/xagora --shell /usr/sbin/nologin xagora
fi

install -d -m 0755 "$env_dir"
install -d -o xagora -g xagora -m 0750 /var/lib/xagora
install -m 0755 "$binary" /usr/local/bin/xagora

if [[ ! -f "$env_file" ]]; then
  cat >"$env_file" <<'ENV'
DATABASE_URL=postgres://xagora:change-me@127.0.0.1:5432/xagora
XAGORA_BIND=0.0.0.0:2222
XAGORA_WORLD=/opt/agentopia/worlds/sample
XAGORA_HOST_KEY=/var/lib/xagora/ssh_host_ed25519_key
XAGORA_ADMIN_SOCKET=/run/xagora/admin.sock
XAGORA_MAIL_DOMAIN=xagora.local
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
Description=Xagora SSH daemon
After=network-online.target postgresql.service
Wants=network-online.target
Requires=postgresql.service

[Service]
Type=simple
User=xagora
Group=xagora
WorkingDirectory=/opt/agentopia
EnvironmentFile=/etc/xagora/xagora.env
RuntimeDirectory=xagora
RuntimeDirectoryMode=0755
StateDirectory=xagora
StateDirectoryMode=0750
ExecStart=/usr/local/bin/xagora serve ssh --bind ${XAGORA_BIND} --world ${XAGORA_WORLD} --host-key ${XAGORA_HOST_KEY} --admin-socket ${XAGORA_ADMIN_SOCKET}
Restart=always
RestartSec=3
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
UNIT

systemctl daemon-reload
systemctl enable xagora.service

printf 'Installed xagora service with binary: /usr/local/bin/xagora\n'
