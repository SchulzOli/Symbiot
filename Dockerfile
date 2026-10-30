# syntax=docker/dockerfile:1
# Symbiot server image: axum server + the Leptos UI, for the home server `heimserver`.

FROM rust:1-bookworm AS build
ARG TRUNK_VERSION=0.22.1
RUN curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin
WORKDIR /src
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
