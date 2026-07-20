# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

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

The `ENV=testing` environment variable disables file-based logging in tests so output stays on console. Tests load `.env` via `dotenvy` and run single-threaded to avoid concurrent access issues. The MQTT subscriber uses a unique test client id per run, and the `mosquitto_pub` helper reads `MQTT_PUBLISH_USER` / `MQTT_PUBLISH_PASSWORD` with local defaults (`device_pubsub` / `DevicePassword1!`).

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
- **`amqp/`** — RabbitMQ client wrapper around `lapin`. Manages connection → channel → queue lifecycle with builder pattern. The named shared queue is declared durable because RabbitMQ 4.x denies transient non-exclusive queues by default. Publishes use the AMQP default exchange (`exchange=""`) with `AMQP_QUEUE_NAME` as the routing key, so the producer RabbitMQ user needs write permission on `amq.default` as well as permissions for the queue name. Channels enable publisher confirms; each publish waits for broker `Ack`, treats `Nack`/returned messages as errors, and retries once after rebuilding the AMQP connection.
- **`models/`** — Typed sensor data models. `Topic` parses MQTT topic strings (`sensors/{deviceId}/{featureName}`). `Notification<T>` is the inbound MQTT payload and must contain `deviceUuid`, `featureUuid`, `timestamp`, `nonce`, `signature`, and `payload`; legacy `apiToken` is no longer forwarded. `Message<T>` is the outbound AMQP payload. `PayloadTrait` defines sensor types: Temperature, Humidity, Light, Motion, AirQuality, AirPressure, Mode, Online. Float sensors: temperature, humidity, light, airpressure, mode (restricted to -1.0, 0.0, 1.0, or 2.0). Integer sensors: motion, airquality. Boolean sensors: online.
- **`errors/`** — Custom error types (`AmqpError`, `MqttError`, `MessageError`) using `thiserror`.
- **`tests_integration/`** — Integration tests (declared in `main.rs` via `#[cfg(test)]`). Requires real MQTT and RabbitMQ brokers.

**Replay boundary:** Producer-generated AMQP `message_id` values are operational metadata only. Security replay protection is based on the signed MQTT `nonce` and is enforced downstream by `consumer` / `online-receiver` using Redis `SET NX EX`.

### Environment Variables

All runtime config is in the `Env` struct (`config/mod.rs`): `AMQP_URI`, `AMQP_HMAC_SECRET`, `AMQP_QUEUE_NAME`, `MQTT_URL`, `MQTT_PORT`, `MQTT_CLIENT_ID`, `MQTT_AUTH`, `MQTT_USER`, `MQTT_PASSWORD`, `MQTT_TLS`, `ROOT_CA`, `MQTT_CERT_FILE`, `MQTT_KEY_FILE`.

`.env_template` also contains `LOG_LEVEL`, but the current code does not read it directly; logging is configured in `config::init()`.

## Rust Edition & Formatting

- Rust edition 2024, resolver 3
- Formatting: 4 spaces, 120 char max width (see `rustfmt.toml`)

## Docker

Multi-stage Dockerfile using `cargo-chef` for dependency caching. Final stage uses a hardened base image (`dhi.io/debian-base:trixie`) running as non-root (uid 65534). Secrets are injected at runtime via Kubernetes Secrets or environment variables; no `.env` file is baked into the image.

## Security & Maintenance

See `CHANGELOG.md` for a comprehensive log of security fixes and recent improvements. Notable recent work includes HMAC-SHA256 message authentication, exponential backoff hardening, and secret zeroing.

### Known Open Issues

The following remain unresolved:

1. **AMQP connection does not use TLS** — no `amqps://` support; bridged sensor messages travel unencrypted over RabbitMQ unless protected by the surrounding network. Requires infrastructure support for AMQP TLS in the deployment.

2. **Combined CA file written to CWD and never deleted** — `mqtt_options.rs` writes `rootca_and_cert.pem` at startup and never removes it after connection.

3. **Partial path-traversal protection for TLS paths** — `mqtt_options.rs` validates `key_store` and `private_key` paths in `build_ssl_options()`, but `merge_ca_files()` still reads `root_ca` and `mqtt_cert_file` directly.
