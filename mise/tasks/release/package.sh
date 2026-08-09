#!/usr/bin/env bash
#MISE description="Build and package a release archive for a target triple"
#USAGE flag "--target <target>" help="Rust target triple to compile"
#USAGE flag "--version <version>" help="Version number to include in the asset name"
set -euo pipefail

target=""
version=""
while (($# > 0)); do
  case "$1" in
    --target)
      target="${2}"
      shift 2
      ;;
    --version)
      version="${2}"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ -z "${target}" || -z "${version}" ]]; then
  echo "--target and --version are required" >&2
  exit 1
fi

mkdir -p dist
cargo build --locked --release --package plasma --target "${target}"

case "${target}" in
  *-windows-*) bin_name="plasma.exe" ;;
  *) bin_name="plasma" ;;
esac

stage_dir="$(mktemp -d)"
trap 'rm -rf "${stage_dir}"' EXIT
cp "target/${target}/release/${bin_name}" "${stage_dir}/${bin_name}"

asset="plasma-${version}-${target}.tar.gz"
tar -C "${stage_dir}" -czf "dist/${asset}" "${bin_name}"

if command -v sha256sum >/dev/null 2>&1; then
  (cd dist && sha256sum "${asset}" > "${asset}.sha256")
else
  (cd dist && shasum -a 256 "${asset}" > "${asset}.sha256")
fi
