# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**producer** is part of the [home-anthill](https://github.com/home-anthill/docs) IoT platform. It is a Rust service that bridges MQTT and AMQP (RabbitMQ): it subscribes to MQTT sensor topics, deserializes incoming payloads into typed models, and republishes them as byte messages onto a RabbitMQ queue. The Cargo package is `ks89-producer`; the binary and library crate are both named `producer`.

## Setup

Copy `.env_template` to `.env` and update credentials/secrets:
```bash
cp .env_template .env
# Edit .env with your MQTT credentials, RabbitMQ URI, and AMQP_HMAC_SECRET
```

Install dev dependencies once:
```bash
make deps          # Install clippy, rustfmt, cargo-watch, cargo-audit
make deps-test     # Install grcov and llvm-tools-preview for coverage
```

## Build & Development Commands

All common workflows are in the `Makefile`:

- **Build:** `make build` (runs fmt + clippy + cargo build; this is the default target)
- **Release build:** `make release` (optimized binary with LTO and single codegen unit)
- **Format:** `make fmt`
- **Lint:** `make lint` (clippy)
- **Security audit:** `make check` (cargo audit for known vulnerabilities)
- **Run (watch mode):** `make run` (requires `cargo-watch`; watches for changes except integration tests)
- **Test:** `make test` — runs with `ENV=testing RUST_BACKTRACE=full`, single-threaded (`--test-threads 1`)
- **Test coverage:** `make test-coverage` (requires `grcov`; generates HTML report in `coverage/html`)
- **Clean build artifacts:** `make clean`
- **Generate docs:** `make doc`
- **Run a single test:** `ENV=testing RUST_BACKTRACE=full cargo test <test_name> -- --nocapture --test-threads 1`

### Test Infrastructure

Integration tests require **real** running infrastructure (no mocks):
- **MQTT broker** — Mosquitto with authentication (see `.env` for credentials)
- **RabbitMQ** — AMQP broker
- **CLI tools** — `mosquitto_pub` must be in `$PATH` for the `receive_message_via_mqtt` test

The `ENV=testing` environment variable disables file-based logging in tests so output stays on console. Tests load `.env` via `dotenvy` and run single-threaded to avoid concurrent access issues.

## Architecture

### Data Flow

```
MQTT Broker → MqttClient (subscribe to sensor topics) → deserialize payload → serialize as Message → AmqpClient (publish to RabbitMQ queue)
```

### Reconnection Strategy

The MQTT client uses a **persistent (non-clean) session** (`clean_session(false)`) so the broker retains subscriptions and queued messages across reconnects. On reconnect, the code explicitly re-subscribes anyway (`mqtt_client.subscribe(topics)`) to handle brokers that discard the session (e.g. session expiry or clean-session mismatch).

**Initial connection** (`main.rs` lines 54–72) retries with exponential backoff:
- Starts at 2 seconds, doubles each attempt, caps at 5 minutes
- Gives up and exits after 10 minutes of continuous failure so Kubernetes can restart

**Reconnection on disconnect** also retries with the same exponential backoff policy. Both policies are symmetric to ensure healthy recovery after broker restarts.

### Project Structure

This project is both a **binary** (`producer` — the bridge service) and a **library** (`lib.rs` re-exports modules for integration tests).

### Key Modules

- **`main.rs`** — Binary entry point. Initializes config, connects to RabbitMQ and MQTT, runs the message loop, and handles reconnection with exponential backoff.
- **`lib.rs`** — Library root; re-exports `amqp`, `config`, `errors`, `models`, and `mqtt` for use in tests.
- **`config/`** — Loads env vars into the `Env` struct using `dotenvy` + `envy`. Sets up `tracing` with rolling file appenders (info + error logs to `./logs/`).
- **`mqtt/`** — MQTT client wrapper around `paho-mqtt`. `MqttConfig` builds connection parameters from `Env`, `MqttOptions` creates paho connection/create options (supports TLS with CA + client certs), `MqttClient` manages connect/subscribe/reconnect.
- **`amqp/`** — RabbitMQ client wrapper around `lapin`. Manages connection → channel → queue lifecycle with builder pattern. Supports auto-recovery on publish failure.
- **`models/`** — Typed sensor data models. `Topic` parses MQTT topic strings (`sensors/{deviceId}/{featureName}`). `Notification<T>` is the inbound MQTT payload. `Message<T>` is the outbound AMQP payload. `PayloadTrait` defines sensor types: Temperature, Humidity, Light, Motion, AirQuality, AirPressure, Online. Float sensors: temperature, humidity, light, airpressure. Integer sensors: motion, airquality. Boolean sensors: online.
- **`errors/`** — Custom error types (`AmqpError`, `MqttError`, `MessageError`) using `thiserror`.
- **`tests_integration/`** — Integration tests (declared in `main.rs` via `#[cfg(test)]`). Requires real MQTT and RabbitMQ brokers.

### Environment Variables

All runtime config is in the `Env` struct (`config/mod.rs`): `AMQP_URI`, `AMQP_HMAC_SECRET`, `AMQP_QUEUE_NAME`, `MQTT_URL`, `MQTT_PORT`, `MQTT_CLIENT_ID`, `MQTT_AUTH`, `MQTT_USER`, `MQTT_PASSWORD`, `MQTT_TLS`, `ROOT_CA`, `MQTT_CERT_FILE`, `MQTT_KEY_FILE`.

`.env_template` also contains `LOG_LEVEL`, but the current code does not read it directly; logging is configured in `config::init()`.

## Rust Edition & Formatting

- Rust edition 2024, resolver 3
- Formatting: 4 spaces, 120 char max width (see `rustfmt.toml`)

## Docker

Multi-stage Dockerfile using `cargo-chef` for dependency caching. Final stage uses a hardened base image (`dhi.io/debian-base:trixie`) running as non-root (uid 65534). Secrets are injected at runtime via Kubernetes Secrets or environment variables; no `.env` file is baked into the image.

## Security & Maintenance

See `CHANGELOG_CLAUDE.md` for a comprehensive log of security fixes and recent improvements. Notable recent work includes HMAC-SHA256 message authentication, exponential backoff hardening, and secret zeroing.

### Known Open Issues

The following remain unresolved:

1. **Hardcoded MQTT credentials in integration tests** — `src/tests_integration/tests.rs` passes `-u mosquser -P Password1!` directly to `mosquitto_pub`. This can drift from `.env_template`/`.env` and should load from `Env` instead.

2. **`.env_template` contains example credentials** — `MQTT_USER` and `MQTT_PASSWORD` are set to concrete example values; should use clearly marked placeholders (`<your-mqtt-user>`, etc.) so developers know they must be replaced.

3. **AMQP connection does not use TLS** — no `amqps://` support; messages (including `api_token`) travel unencrypted over RabbitMQ. Requires infrastructure support for AMQP TLS in the deployment.

4. **Combined CA file written to CWD and never deleted** — `mqtt_options.rs` writes `rootca_and_cert.pem` at startup and never removes it after connection.

5. **Partial path-traversal protection for TLS paths** — `mqtt_options.rs` validates `key_store` and `private_key` paths in `build_ssl_options()`, but `merge_ca_files()` still reads `root_ca` and `mqtt_cert_file` directly.
