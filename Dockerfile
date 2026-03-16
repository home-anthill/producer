# syntax=docker/dockerfile:1

# ── Stage 1: Chef setup ──────────────────────────────────────────────────────
FROM rust:trixie AS chef

# some cargo dependencies require additional packages to build the project.
RUN apt-get update && apt-get install -y \
    g++ \
    openssl \
    make cmake

WORKDIR /app

RUN cargo install cargo-chef


# ── Stage 2: Planner ─────────────────────────────────────────────────────────
FROM chef AS planner

COPY . .

RUN cargo chef prepare --recipe-path recipe.json


# ── Stage 3: Builder ─────────────────────────────────────────────────────────
FROM chef AS builder

WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json

# copy source code and build it
COPY . .

RUN cargo build --release


# ── Stage 4: Collect CA certs and runtime shared libraries ───────────────────
# The hardened runtime image has no package manager, so we install here and
# copy what we need into the final stage.
FROM debian:trixie-slim AS system-deps

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libstdc++6 \
    libgcc-s1

# Collect runtime libs into a flat staging directory so the final COPY
# works regardless of build architecture (x86_64 vs aarch64).
RUN mkdir -p /staging/lib && \
    cp /usr/lib/*-linux-gnu/libssl.so.3    /staging/lib/ && \
    cp /usr/lib/*-linux-gnu/libcrypto.so.3 /staging/lib/ && \
    cp /usr/lib/*-linux-gnu/libstdc++.so.6 /staging/lib/ && \
    cp /usr/lib/*-linux-gnu/libgcc_s.so.1  /staging/lib/

# Pre-create the app directory owned by nobody (uid/gid 65534) so the final
# stage never needs to run a RUN command as root.
RUN mkdir -p /app/logs && chown -R 65534:65534 /app


# ── Stage 5: Hardened runtime ────────────────────────────────────────────────
# dhi.io/debian-base:trixie — no package manager, no root user, shell present.
FROM dhi.io/debian-base:trixie AS runtime

# CA certificates for TLS (includes ISRG_Root_X1.pem used in prod).
COPY --from=system-deps /etc/ssl/certs /etc/ssl/certs

# Runtime shared libraries absent from the hardened base image.
# paho-mqtt (SSL variant) dynamically links against OpenSSL; the C++ runtime
# is pulled in by the paho C library build.
COPY --from=system-deps /staging/lib/ /usr/lib/

# App directory skeleton (/app and /app/logs owned by nobody).
COPY --from=system-deps --chown=65534:65534 /app /app

WORKDIR /app

# Binary and env template.
COPY --from=builder --chown=65534:65534 /app/target/release/producer /app/producer
COPY --from=builder --chown=65534:65534 /app/.env_template /.env

USER 65534

ENTRYPOINT ["/app/producer"]
