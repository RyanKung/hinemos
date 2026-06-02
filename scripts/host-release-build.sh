#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage: agentopia-release-build [repo]

Build xagora on the host in release mode, copy the final binary to
<repo>/.host-build/xagora, then remove temporary cargo build files.
USAGE
  exit 0
fi

repo="${1:-/opt/agentopia}"
target_dir="${CARGO_TARGET_DIR:-$repo/.cargo-target}"
output_dir="${AGENTOPIA_HOST_BUILD_DIR:-$repo/.host-build}"
export CARGO_TARGET_DIR="$target_dir"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

cleanup() {
  rm -rf "$target_dir"
  rm -rf "$repo/target/debug" "$repo/target/tmp"
}
trap cleanup EXIT

cd "$repo"
mkdir -p "$output_dir"

cargo build --release --bin xagora --jobs "$CARGO_BUILD_JOBS"
install -m 0755 "$target_dir/release/xagora" "$output_dir/xagora"

printf 'Built release binary: %s\n' "$output_dir/xagora"
