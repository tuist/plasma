#!/usr/bin/env bash
#MISE description="Detect whether the current branch has releasable changes"
set -euo pipefail

latest_version="$(git cliff --config cliff.toml --repository . --latest --bumped-version)"
next_version="$(git cliff --config cliff.toml --repository . --unreleased --bumped-version)"

should_release=false
if [[ "${next_version}" != "${latest_version}" ]]; then
  should_release=true
fi

echo "latest-version=${latest_version:-none}"
echo "next-version=${next_version}"
echo "should-release=${should_release}"

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "latest-version=${latest_version}"
    echo "next-version=${next_version}"
    echo "should-release=${should_release}"
  } >> "${GITHUB_OUTPUT}"
fi
