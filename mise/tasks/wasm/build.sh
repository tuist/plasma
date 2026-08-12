#!/usr/bin/env bash
#MISE description="Build the WebAssembly package and stage it for the web application"
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
package_root="${repository_root}/packages/plasma"
web_static_root="${repository_root}/web/priv/static/plasma"

wasm-pack build "${repository_root}/crates/plasma-wasm" \
  --target web \
  --out-dir "${package_root}/generated" \
  --out-name plasma_wasm \
  --release

mkdir -p "${web_static_root}/generated"
cp "${package_root}/index.js" "${web_static_root}/index.js"
cp "${package_root}/generated/plasma_wasm.js" "${web_static_root}/generated/plasma_wasm.js"
cp "${package_root}/generated/plasma_wasm_bg.wasm" "${web_static_root}/generated/plasma_wasm_bg.wasm"
