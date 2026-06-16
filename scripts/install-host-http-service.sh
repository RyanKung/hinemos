#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage: install-host-http-service.sh [repo]

Install the host-built hinemos binary and HTTP systemd service.
Expected binary: <repo>/.host-build/hinemos
Expected frontend: <repo>/web/landing/dist/index.html
Expected env file: /etc/hinemos/hinemos.env
USAGE
  exit 0
fi

repo="${1:-/opt/hinemos}"
binary="$repo/.host-build/hinemos"
frontend_index="$repo/web/landing/dist/index.html"
service_file="/etc/systemd/system/hinemos-http.service"
env_dir="/etc/hinemos"
env_file="$env_dir/hinemos.env"

if [[ $EUID -ne 0 ]]; then
  echo "install-host-http-service.sh must run as root" >&2
  exit 1
fi

if [[ ! -x "$binary" ]]; then
  echo "missing executable build artifact: $binary" >&2
  echo "run: scripts/host-release-build.sh $repo" >&2
  exit 1
fi

if [[ ! -f "$frontend_index" ]]; then
  echo "missing frontend build artifact: $frontend_index" >&2
  echo "run: (cd $repo/web/landing && trunk build --release)" >&2
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
HINEMOS_WORLD=/opt/hinemos/worlds/sample
HINEMOS_HTTP_BIND=127.0.0.1:8080
HINEMOS_HTTP_STATIC_DIR=/opt/hinemos/web/landing/dist
ENV
  chmod 0600 "$env_file"
  chown root:root "$env_file"
else
  if ! grep -q '^HINEMOS_WORLD=' "$env_file"; then
    echo 'HINEMOS_WORLD=/opt/hinemos/worlds/sample' >>"$env_file"
  fi
  if grep -q '^HINEMOS_HTTP_BIND=' "$env_file"; then
    sed -i 's#^HINEMOS_HTTP_BIND=.*#HINEMOS_HTTP_BIND=127.0.0.1:8080#' "$env_file"
  else
    echo 'HINEMOS_HTTP_BIND=127.0.0.1:8080' >>"$env_file"
  fi
  if ! grep -q '^HINEMOS_HTTP_STATIC_DIR=' "$env_file"; then
    echo 'HINEMOS_HTTP_STATIC_DIR=/opt/hinemos/web/landing/dist' >>"$env_file"
  fi
fi

cat >"$service_file" <<'UNIT'
[Unit]
Description=Hinemos HTTP landing and API service
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=hinemos
Group=hinemos
WorkingDirectory=/opt/hinemos
EnvironmentFile=/etc/hinemos/hinemos.env
ExecStart=/usr/local/bin/hinemos serve http --bind ${HINEMOS_HTTP_BIND} --world ${HINEMOS_WORLD} --static-dir ${HINEMOS_HTTP_STATIC_DIR}
Restart=always
RestartSec=3
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
UNIT

systemctl daemon-reload
systemctl enable hinemos-http.service

printf 'Installed hinemos-http service with binary: /usr/local/bin/hinemos\n'
printf 'Start with: systemctl restart hinemos-http.service\n'
