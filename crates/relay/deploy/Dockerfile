# syntax=docker/dockerfile:1.7
# ome-node: ome-relay and ome-keeper in one image. Relay operators and members running their keeper on a server use
# the same image. Build from `crates/`:
#
#   docker build -f relay/deploy/Dockerfile -t ome-node .

FROM rust:1.99.1-slim-trixie AS build
WORKDIR /src
# Crate downloads stall on some networks over HTTP/2; use HTTP/1.1 with generous retries and timeouts.
ENV CARGO_HTTP_MULTIPLEXING=false CARGO_NET_RETRY=10 CARGO_HTTP_TIMEOUT=120
COPY . .
# Fetch separately so an interrupted download resumes from the cache instead of restarting the build.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo fetch --locked
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --offline -p ome-keeper -p ome-relay --bins \
 && mkdir -p /out \
 && cp target/release/ome-keeper target/release/ome-relay /out/

FROM debian:trixie-slim AS runtime
RUN useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin ome \
 && mkdir -p /data && chown ome:ome /data && chmod 0700 /data
COPY --from=build /out/ome-keeper /out/ome-relay /usr/local/bin/
USER ome
WORKDIR /data
VOLUME /data
# libp2p (QUIC and TCP) and the relay's health and metrics endpoint.
EXPOSE 4001/udp 4001/tcp 9090/tcp
# Debian's /bin/sh is dash, which has no /dev/tcp; bash does.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD ["/bin/bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/9090 && printf 'GET /health HTTP/1.0\\r\\n\\r\\n' >&3 && grep -q '\"status\":\"ok\"' <&3"]
CMD ["ome-relay", "run", "--data-dir", "/data", "--health", "0.0.0.0:9090", "--log-format", "json"]
