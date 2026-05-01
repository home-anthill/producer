# Changelog (Claude)

## 2026-05-01 Documentation Refresh

- **Claude guidance aligned with current config** — `CLAUDE.md` now includes `AMQP_HMAC_SECRET` in the documented runtime environment and notes that `.env_template` has `LOG_LEVEL`, although the current `Env` struct does not consume it.
- **Project naming clarified** — Documented the distinction between the Cargo package name (`ks89-producer`) and the binary/library crate name (`producer`).
- **Open issues refreshed** — Updated stale paths and credential values in the integration-test credential issue, clarified that `.env_template` still contains concrete example credentials, and corrected the TLS CA-file/path-traversal notes to match `mqtt_options.rs`.

## Security

- **Credential redaction** — AMQP URIs are scrubbed of embedded credentials before logging (`redact_uri()`). MQTT username and password are printed as `[REDACTED]` in logs and debug output. A hand-written `fmt::Debug` for `Env` redacts all four secret fields (`amqp_uri`, `amqp_hmac_secret`, `mqtt_user`, `mqtt_password`).
- **Secret zeroing** — `amqp_uri`, `amqp_hmac_secret`, `mqtt_user`, and `mqtt_password` are wrapped in `Zeroizing<String>` in both `Env` and `AmqpClient`, so secrets are wiped from memory on drop.
- **HMAC-SHA256 message authentication** — Every AMQP message carries an `x-hmac-sha256` header computed from the payload using `AMQP_HMAC_SECRET`, allowing consumers to verify integrity and origin. The MQTT Last-Will-and-Testament payload is similarly signed so consumers can detect spoofed disconnect notifications.
- **Replay-attack mitigations** — Each published AMQP message includes a `uuid::Uuid::new_v4()` as `message_id` and a Unix timestamp in `BasicProperties`. A 30-second per-message TTL (`.with_expiration("30000")`) limits the effective replay window; RabbitMQ dead-letters expired messages automatically.
- **Input validation** — `Topic::new()` enforces a 255-byte length cap per segment, requires `device_id` to be a valid non-nil UUID, and whitelists `feature_name` against known sensor types. UUIDs in the MQTT payload (`api_token`, `device_uuid`, `feature_uuid`) are validated and nil-UUID values are rejected. `AMQP_QUEUE_NAME` is checked at startup for length (1–255) and RabbitMQ-safe characters.
- **Payload size limit** — `get_bytes_from_payload()` rejects payloads larger than 65 536 bytes before any processing, preventing memory pressure from malicious publishers.
- **TLS hardening** — `.enable_server_cert_auth(true)` is set on `SslOptionsBuilder` to require broker certificate verification. TLS certificate paths are resolved to absolute paths via `env::current_dir()` and guarded against `..` path-traversal components.
- **Sensitive data removed from logs** — `debug!` calls that logged the full raw MQTT payload (containing `apiToken`) and the full `payload_str` were removed.
- **No secrets in Docker images** — The `.env_template` file is no longer copied into production images; secrets must be injected at runtime via Kubernetes Secrets or environment variables.
- **Startup secret validation** — `AMQP_URI` and `AMQP_HMAC_SECRET` must be non-empty; `MQTT_PASSWORD` must be non-empty when `MQTT_AUTH` is enabled. The process exits cleanly (without exposing credentials in the panic message) on validation failure.
- **No credentials in process termination messages** — Startup and reconnect `panic!` calls that could expose URI or credentials in lapin error messages were replaced with logged errors followed by clean process exit.

## Reliability

- **MQTT QoS upgrade** — Subscriptions upgraded from QoS 0 ("at most once") to QoS 1 ("at least once"). The broker retains messages and retransmits on reconnect; duplicate delivery is harmless because sensor readings are idempotent.
- **Exponential backoff on reconnect** — Both the initial connect loop and the reconnect-on-disconnect loop use exponential backoff starting at 2 s, doubling to a 5-minute cap. After 10 minutes of continuous failure the process exits so Kubernetes can restart it with its own backoff policy.
- **Latent panic fix in `wait_for_recovery`** — Fixed a panic that could occur when the AMQP channel was `None` during recovery.
- **Failed messages now visible in logs** — `let _ = process_mqtt_message(...).await` was replaced with an `if let Err` guard so processing failures are logged instead of silently dropped.

## Idiomatic Rust

- **Error propagation** — `async fn main()` returns `anyhow::Result<()>`, eliminating `std::process::exit` calls from async context; startup failures use `.context(...)?` and reconnect timeouts use `anyhow::bail!`. `MqttOptions::new()` and `AmqpClient::connect()` return `Result` instead of panicking internally. `MqttClient::connect()` returns `Result<(), paho_mqtt::Error>`; the retry loop was moved to `main` so callers control the retry policy.
- **Option/Result combinators** — Replaced `match`/`unwrap_or_else` chains with `?`, `.inspect_err(...).ok()?`, and `.ok_or_else()` / `.map_err()` throughout `main.rs`, `mqtt/mod.rs`, and `amqp/mod.rs`. `get_msg_byte()`, `message_payload_to_bytes()`, `get_bytes_from_payload()`, and `get_string_payload()` all return typed `Option`/`Result` instead of using empty-`vec![]` or empty-string sentinels.
- **Type safety improvements** — Replaced multiple boolean flags in `AmqpClient::is_initialized` with an explicit `InitLevel` enum (`Disconnected` → `Connection` → `Channel` → `Queue` → `Consumer`). `consumer_tag` changed from `ShortString` (empty-string sentinel) to `Option<ShortString>`. `Online` payload type corrected from `i64` to `bool`. `subscribe()` made generic over `S: AsRef<str>`.
- **Serde correctness** — Removed unused `Serialize` from `Notification<T>` (receive-only) and unused `Deserialize` from `Message<T>` (send-only). `Message::new()` made private. Added `deserialize_finite_f64` serde helper that rejects `NaN` and `±Infinity` for float sensor fields; `f64` sensor structs derive `PartialEq`, integer/bool types additionally derive `Eq`. Added `PartialEq`/`Eq` derives to `Topic` and `PartialEq` to `Notification<T>`.
- **Error display** — `AmqpError` variants display their inner context string via `#[error("{0}")]` with descriptive prefixes. Removed the dead `PublishMessageError` variant from `MessageError`.
- **Code hygiene** — Removed `#![allow(clippy::uninlined_format_args)]` from crate roots; all format arguments now use inline syntax (`{var}`). Replaced string concatenation with `format!()`. Replaced `unwrap().unwrap()` chains with `.expect("…")` carrying descriptive messages. Removed the unused `ca_files_path` field from `MqttConfig`. Replaced `env::var("ENV") != Ok("testing".to_string())` with `.as_deref()` to avoid a needless heap allocation. Fixed `clippy::pedantic` lints: redundant closures replaced with method references, raw string hash fixes.

## Tests

- **Assertions aligned with early validation** — `wrong_get_msg_byte_unknown_sensor` and `wrong_sensor_type_for_process_mqtt_message` updated to assert that `Topic::new()` returns `Err` for unknown sensor types, replacing assertions that relied on the now-unreachable downstream path.
- **Test robustness** — Replaced `unwrap().unwrap()` chains with `.expect("…")` in integration tests. Removed unused `MessageError` import. Updated tests for cascading API changes (`connect()`, `MqttOptions::new()`, `get_msg_byte()`).
