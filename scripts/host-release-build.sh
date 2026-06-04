#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage: hinemos-release-build [repo]

Build a release hinemos binary and copy it to <repo>/.host-build/hinemos.
On non-Linux hosts this script uses a Linux cross-compilation target so the
resulting binary can be uploaded to the remote host without rebuilding there.
USAGE
  exit 0
fi

repo="${1:-/opt/hinemos}"
target_dir="${CARGO_TARGET_DIR:-$repo/.cargo-target}"
output_dir="${HINEMOS_HOST_BUILD_DIR:-$repo/.host-build}"
cache_dir="${HINEMOS_CACHE_DIR:-$repo/.cache}"
cargo_home="${CARGO_HOME:-$repo/.cargo-home}"
host_os="$(uname -s)"
release_target="${HINEMOS_RELEASE_TARGET:-}"
export CARGO_HOME="$cargo_home"
export CARGO_TARGET_DIR="$target_dir"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-$cache_dir}"
export CARGO_ZIGBUILD_CACHE_DIR="${CARGO_ZIGBUILD_CACHE_DIR:-$cache_dir/cargo-zigbuild}"

cleanup() {
  rm -rf "$target_dir"
  rm -rf "$repo/target/debug" "$repo/target/tmp"
}
trap cleanup EXIT

cd "$repo"
mkdir -p "$output_dir"
mkdir -p "$cache_dir"
mkdir -p "$cargo_home"
mkdir -p "$CARGO_ZIGBUILD_CACHE_DIR"

if [[ -z "$release_target" && "$host_os" != "Linux" ]]; then
  release_target="${HINEMOS_RELEASE_TARGET:-x86_64-unknown-linux-gnu}"
fi

if [[ -n "$release_target" ]]; then
  if ! command -v cargo-zigbuild >/dev/null 2>&1; then
    echo "missing cargo-zigbuild; install it with: cargo install cargo-zigbuild" >&2
    exit 1
  fi
  if ! command -v zig >/dev/null 2>&1; then
    echo "missing zig; install it with: brew install zig" >&2
    exit 1
  fi
  cargo zigbuild --locked --offline --release --target "$release_target" --bin hinemos --jobs "$CARGO_BUILD_JOBS"
  install -m 0755 "$target_dir/$release_target/release/hinemos" "$output_dir/hinemos"
else
  cargo build --locked --release --bin hinemos --jobs "$CARGO_BUILD_JOBS"
  install -m 0755 "$target_dir/release/hinemos" "$output_dir/hinemos"
fi

printf 'Built release binary: %s\n' "$output_dir/hinemos"
