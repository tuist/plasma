# syntax=docker/dockerfile:1.7

FROM docker.io/library/rust:1.91-slim-bookworm AS wasm-builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-pack --version 0.13.1 --locked

WORKDIR /app
COPY Cargo.toml Cargo.lock LICENSE.md ./
COPY crates ./crates

RUN wasm-pack build crates/plasma-wasm \
    --target web \
    --out-dir /app/plasma-wasm \
    --out-name plasma_wasm \
    --release

FROM docker.io/library/elixir:1.19.5-otp-28-slim AS web-builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential ca-certificates git \
    && rm -rf /var/lib/apt/lists/*

ENV ERL_FLAGS="+JPperf true" \
    MIX_ENV=prod

RUN mix local.hex --force && mix local.rebar --force
WORKDIR /app/web

COPY web/mix.exs web/mix.lock ./
COPY web/config ./config
RUN mix deps.get --only prod && mix deps.compile

COPY web/assets ./assets
COPY web/lib ./lib
COPY web/priv ./priv
COPY web/rel ./rel

RUN chmod 755 rel/overlays/bin/migrate rel/overlays/bin/server

RUN rm -rf priv/static/assets priv/static/plasma priv/static/cache_manifest.json \
    && mkdir -p priv/static/plasma/generated

COPY packages/plasma/index.js ./priv/static/plasma/index.js
COPY --from=wasm-builder /app/plasma-wasm/plasma_wasm.js ./priv/static/plasma/generated/plasma_wasm.js
COPY --from=wasm-builder /app/plasma-wasm/plasma_wasm_bg.wasm ./priv/static/plasma/generated/plasma_wasm_bg.wasm

RUN mix compile \
    && mix esbuild.install --if-missing \
    && mix esbuild plasma_site --minify \
    && mix phx.digest \
    && mix release

FROM docker.io/library/debian:trixie-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libncurses6 libstdc++6 openssl \
    && rm -rf /var/lib/apt/lists/*

ENV HOME=/app \
    LANG=C.UTF-8 \
    PHX_SERVER=true \
    PORT=4000

WORKDIR /app
COPY --from=web-builder --chown=nobody:nogroup /app/web/_build/prod/rel/plasma_site ./

USER nobody
EXPOSE 4000

CMD ["/app/bin/server"]
