# syntax=docker/dockerfile:1.7
# The relay and keeper images, ghcr.io/ome-labs/ome-relay and ghcr.io/ome-labs/ome-keeper. Build from `crates/`:
#
#   docker build -f relay/deploy/Dockerfile --target relay -t ome-relay .
#   docker build -f relay/deploy/Dockerfile --target keeper -t ome-keeper .

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

FROM debian:trixie-slim AS base
RUN useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin ome \
 && mkdir -p /data && chown ome:ome /data && chmod 0700 /data
USER ome
WORKDIR /data
VOLUME /data

# A member's keeper on their own server. The passphrase that seals its secrets comes from a file, e.g. a Docker
# secret: OME_KEEPER_PASSPHRASE_FILE=/run/secrets/keeper-passphrase. Port 7445 serves the RPC over TLS for the
# member's daemon; the certificate fingerprint is in the logs and /data/keeper.json.
FROM base AS keeper
COPY --from=build /out/ome-keeper /usr/local/bin/
ENV OME_KEEPER_DATA=/data
EXPOSE 4001/udp 4001/tcp 7445/tcp
CMD ["ome-keeper", "run", "--listen", "/ip4/0.0.0.0/udp/4001/quic-v1", "--listen", "/ip4/0.0.0.0/tcp/4001", "--rpc-listen", "0.0.0.0:7445", "--log-format", "json"]

# The relay (the default target): circuits and mailboxes for members' keepers, ciphertext only.
FROM base AS relay
COPY --from=build /out/ome-relay /usr/local/bin/
# libp2p (QUIC and TCP) and the health and metrics endpoint.
EXPOSE 4001/udp 4001/tcp 9090/tcp
# Debian's /bin/sh is dash, which has no /dev/tcp; bash does.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD ["/bin/bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/9090 && printf 'GET /health HTTP/1.0\\r\\n\\r\\n' >&3 && grep -q '\"status\":\"ok\"' <&3"]
CMD ["ome-relay", "run", "--data-dir", "/data", "--health", "0.0.0.0:9090", "--log-format", "json"]
