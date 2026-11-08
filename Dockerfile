# syntax=docker/dockerfile:1
# Symbiot server image: axum server + the Leptos UI, for the home server.

FROM rust:1-bookworm AS build
ARG TRUNK_VERSION=0.22.1
RUN curl -fsSL --retry 6 --retry-all-errors --retry-delay 10 "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin
WORKDIR /src
# trunk lädt wasm-bindgen und wasm-opt sonst bei jedem Build von GitHub-Releases. Diese Schicht legt sie
# vorab in den Cache von trunk und hängt nur an Cargo.lock und Trunk.toml, bleibt also meist im Build-Cache.
COPY Cargo.lock ./
COPY apps/ui/Trunk.toml apps/ui/
RUN set -eu; \
    get() { curl -fsSL --retry 6 --retry-all-errors --retry-delay 10 "$1"; }; \
    WB=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p'); \
    WO=$(sed -n 's/^wasm_opt = "\(.*\)"$/\1/p' apps/ui/Trunk.toml); \
    C=/root/.cache/trunk; mkdir -p "$C/wasm-bindgen-$WB" "$C/wasm-opt-$WO"; \
    get "https://github.com/rustwasm/wasm-bindgen/releases/download/$WB/wasm-bindgen-$WB-x86_64-unknown-linux-musl.tar.gz" \
      | tar -xz --strip-components=1 -C "$C/wasm-bindgen-$WB" "wasm-bindgen-$WB-x86_64-unknown-linux-musl/wasm-bindgen"; \
    get "https://github.com/WebAssembly/binaryen/releases/download/$WO/binaryen-$WO-x86_64-linux.tar.gz" \
      | tar -xz --strip-components=1 -C "$C/wasm-opt-$WO" "binaryen-$WO/bin/wasm-opt"
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cd apps/ui && trunk build --release \
    && cd /src && cargo build --release --locked -p symbiot-server \
    && cp target/release/symbiot-server /symbiot-server \
    && mkdir /data

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /symbiot-server /symbiot-server
COPY --from=build /src/apps/ui/dist /ui
COPY --from=build --chown=nonroot:nonroot /data /data
ENV SYMBIOT_ADDR=0.0.0.0:8100 \
    SYMBIOT_DATA=/data \
    SYMBIOT_UI=/ui \
    RUST_LOG=info
VOLUME /data
EXPOSE 8100
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD ["/symbiot-server", "--health"]
ENTRYPOINT ["/symbiot-server"]
