#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage: hinemos-release-build [repo]

Build hinemos on the host in release mode, copy the final binary to
<repo>/.host-build/hinemos, then remove temporary cargo build files.
USAGE
  exit 0
fi

repo="${1:-/opt/hinemos}"
target_dir="${CARGO_TARGET_DIR:-$repo/.cargo-target}"
output_dir="${HINEMOS_HOST_BUILD_DIR:-$repo/.host-build}"
export CARGO_TARGET_DIR="$target_dir"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

cleanup() {
  rm -rf "$target_dir"
  rm -rf "$repo/target/debug" "$repo/target/tmp"
}
trap cleanup EXIT

cd "$repo"
mkdir -p "$output_dir"

cargo build --release --bin hinemos --jobs "$CARGO_BUILD_JOBS"
install -m 0755 "$target_dir/release/hinemos" "$output_dir/hinemos"

printf 'Built release binary: %s\n' "$output_dir/hinemos"
