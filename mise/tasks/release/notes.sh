#!/usr/bin/env bash
#MISE description="Generate release notes for the next release"
#USAGE flag "--version <version>" help="Version number to render into the generated notes"
set -euo pipefail

version=""
while (($# > 0)); do
  case "$1" in
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

if [[ -z "${version}" ]]; then
  echo "--version is required" >&2
  exit 1
fi

rendered="$(git cliff --config cliff.toml --repository . --unreleased --tag "${version}")"

awk '
  !found && /<!-- RELEASE NOTES START -->/ { found = 1; next }
  found
' <<<"${rendered}"
