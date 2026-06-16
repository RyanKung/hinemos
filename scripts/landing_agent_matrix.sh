#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
LANDING_DIR="$ROOT/web/landing"
LANDING_DIST="$LANDING_DIR/dist"
ENV_LOADED=0

load_env_file() {
  local env_path="$1"
  [[ -f "$env_path" ]] || return 0
  while IFS= read -r line; do
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"
    [[ -z "$line" || "$line" == \#* ]] && continue
    line="${line#export }"
    local key="${line%%=*}"
    local value="${line#*=}"
    key="${key#"${key%%[![:space:]]*}"}"
    key="${key%"${key##*[![:space:]]}"}"
    value="${value#"${value%%[![:space:]]*}"}"
    value="${value%"${value##*[![:space:]]}"}"
    if [[ "$value" == \"*\" && "$value" == *\" ]]; then
      value="${value:1:${#value}-2}"
    elif [[ "$value" == \'*\' && "$value" == *\' ]]; then
      value="${value:1:${#value}-2}"
    fi
    if [[ -n "$key" && -z "${!key:-}" ]]; then
      export "$key=$value"
    fi
  done <"$env_path"
  ENV_LOADED=1
}

load_local_env() {
  load_env_file "$ROOT/.env"
  load_env_file "$ROOT/.env.test"
  load_env_file "$ROOT/.env.local"
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'missing required command: %s\n' "$1" >&2
    exit 1
  }
}

need_cmd cargo
need_cmd claude
need_cmd python3
need_cmd trunk

load_local_env

if [[ -z "${ANTHROPIC_BASE_URL:-}" || -z "${ANTHROPIC_AUTH_TOKEN:-}" ]]; then
  cat >&2 <<'EOF'
ANTHROPIC_BASE_URL and ANTHROPIC_AUTH_TOKEN must be set in the shell.
This script intentionally uses the caller's environment or local .env files
instead of starting or logging into rotom itself.
EOF
  exit 1
fi

build_landing() {
  (cd "$LANDING_DIR" && env -u NO_COLOR trunk build --release)
}

start_http_server() {
  local port="$1"
  python3 -m http.server "$port" --bind 127.0.0.1 --directory "$LANDING_DIST" >/tmp/hinemos-landing-matrix-http.log 2>&1 &
  echo $!
}

run_case() {
  local model="$1"
  local landing_url="$2"
  local llm_url="$3"
  local prompt output status

  prompt=$(cat <<EOF
You are a first-time user on the Hinemos landing page.
Read the landing page at ${landing_url} and the machine-readable guide at ${llm_url}.
Return exactly four lines:
NEXT=<short next step>
ED25519=<exact ed25519 key command>
GUIDE=<preferred guide path>
WEB=<entrance|main|unclear>

Rules:
- The website is only the entrance, not the main interface.
- The ed25519 command must be the one shown on the page.
- The guide path must be /llm.txt.
EOF
)

  set +e
  output="$(
    ANTHROPIC_MODEL="$model" \
    ANTHROPIC_BASE_URL="$ANTHROPIC_BASE_URL" \
    ANTHROPIC_AUTH_TOKEN="$ANTHROPIC_AUTH_TOKEN" \
    claude -p "$prompt" \
      --output-format text \
      --print \
      --permission-mode bypassPermissions \
      --allowedTools "Bash(curl *),Bash(cat *),Bash(grep *),Bash(sed *),Bash(awk *),Bash(head *),Bash(tail *)"
  )"
  status=$?
  set -e

  printf '\n[%s]\n%s\n' "$model" "$output"
  if [[ $status -ne 0 ]]; then
    printf 'model %s exited with status %d\n' "$model" "$status" >&2
  fi

  if [[ $status -ne 0 && "$output" != *"NEXT="* ]]; then
    printf 'model %s skipped: no usable provider response in this environment\n' "$model" >&2
    return 0
  fi

  local next_line ed25519_line guide_line web_line
  next_line="$(grep -i '^NEXT=' <<<"$output" | tail -n1)"
  ed25519_line="$(grep -i '^ED25519=' <<<"$output" | tail -n1)"
  guide_line="$(grep -i '^GUIDE=' <<<"$output" | tail -n1)"
  web_line="$(grep -i '^WEB=' <<<"$output" | tail -n1)"

  if [[ "${next_line,,}" != *ssh* && "${next_line,,}" != *connect* ]]; then
    printf 'model %s did not identify SSH as the next step\n' "$model" >&2
    return 1
  fi

  if [[ "${ed25519_line,,}" != *ssh-keygen* || "${ed25519_line,,}" != *ed25519* || "${ed25519_line,,}" != *hinemos* ]]; then
    printf 'model %s did not produce the ed25519 command\n' "$model" >&2
    return 1
  fi

  if [[ "${guide_line,,}" != guide=/llm.txt ]]; then
    printf 'model %s did not prefer /llm.txt\n' "$model" >&2
    return 1
  fi

  if [[ "${web_line,,}" != web=entrance ]]; then
    printf 'model %s misclassified the website role\n' "$model" >&2
    return 1
  fi
}

main() {
  local http_pid http_port landing_url llm_url
  local models=(
    "gpt-5.5"
    "claude-opus-4.8"
    "grok-4.3"
    "cursor/sonnet-4"
    "deepseek-3.2"
    "glm-5"
    "qwen3-coder-next"
  )

  build_landing

  http_pid=""
  http_port="$(
    python3 - <<'PY'
import socket
sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
  )"
  landing_url="http://127.0.0.1:${http_port}"
  llm_url="${landing_url}/llm.txt"
  http_pid="$(start_http_server "$http_port")"

  cleanup() {
    if [[ -n "${http_pid:-}" ]]; then
      kill "$http_pid" >/dev/null 2>&1 || true
    fi
  }
  trap cleanup EXIT

  local failed=0
  for model in "${models[@]}"; do
    if ! run_case "$model" "$landing_url" "$llm_url"; then
      failed=1
    fi
  done

  return "$failed"
}

main "$@"
