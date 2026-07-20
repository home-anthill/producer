# Changelog

## 4.1.0

### Features

- Added `mode` sensor feature for thermostat

### Tests

- Added AMQP unit coverage for uninitialized clients, connection-state checks, publishing while reconnecting, and closing before a connection exists.
- Added model validation coverage for malformed payload UUIDs, nil UUIDs, and invalid timestamps.
- Added topic parsing rejection cases for malformed topic shapes, empty segments, overlong segments, invalid UUIDs, nil UUIDs, and unknown sensor types.
- Added MQTT payload handling coverage for invalid UTF-8, invalid topics, oversized payloads, and malformed JSON.
- Added MQTT options coverage for Last Will payload JSON/signature generation and TLS option validation failures.


## 4.0.0

### Features

- **Publisher confirms enabled** — AMQP channels now call `confirm_select()` and each publish waits for broker confirmation. Returned messages, `Nack`, missing confirm mode, and confirm failures are treated as publish errors instead of assuming the broker accepted the message.
- **Publish retry tightened** — `publish_message()` now retries a failed publish once after rebuilding the AMQP connection, while preserving the existing initialization guard.
- **Durable queue declaration documented in code path** — Queue declaration now explicitly sets `durable: true`, matching RabbitMQ 4.x behavior for named shared queues.
- **MQTT envelope changed** — Inbound MQTT notifications and outbound AMQP messages no longer carry `apiToken`. The bridge now requires and forwards `timestamp`, `nonce`, and `signature`, dropping messages with missing or invalid signed-envelope fields.
- **MQTT QoS upgrade** — Subscriptions upgraded from QoS 0 ("at most once") to QoS 1 ("at least once"). The broker retains messages and retransmits on reconnect; duplicate delivery is harmless because sensor readings are idempotent.
- **Exponential backoff on reconnect** — Both the initial connect loop and the reconnect-on-disconnect loop use exponential backoff starting at 2 s, doubling to a 5-minute cap. After 10 minutes of continuous failure the process exits so Kubernetes can restart it with its own backoff policy.

### Bug fixes

- **Latent panic fix in `wait_for_recovery`** — Fixed a panic that could occur when the AMQP channel was `None` during recovery.
- **Failed messages now visible in logs** — `let _ = process_mqtt_message(...).await` was replaced with an `if let Err` guard so processing failures are logged instead of silently dropped.

### Security issues

- **Signed envelope validation tightened** — MQTT notifications must carry a 32-character lowercase-hex nonce and 64-character lowercase-hex signature before the producer forwards them to AMQP.
- **Credential redaction** — AMQP URIs are scrubbed of embedded credentials before logging (`redact_uri()`). MQTT username and password are printed as `[REDACTED]` in logs and debug output. A hand-written `fmt::Debug` for `Env` redacts all four secret fields (`amqp_uri`, `amqp_hmac_secret`, `mqtt_user`, `mqtt_password`).
- **Secret zeroing** — `amqp_uri`, `amqp_hmac_secret`, `mqtt_user`, and `mqtt_password` are wrapped in `Zeroizing<String>` in both `Env` and `AmqpClient`, so secrets are wiped from memory on drop.
- **HMAC-SHA256 message authentication** — Every AMQP message carries an `x-hmac-sha256` header computed from the payload using `AMQP_HMAC_SECRET`, allowing consumers to verify integrity and origin. The MQTT Last-Will-and-Testament payload is similarly signed so consumers can detect spoofed disconnect notifications.
- **Replay-attack metadata** — Each published AMQP message includes a fresh `uuid::Uuid::new_v4()` as `message_id` and a Unix timestamp in `BasicProperties`; these are operational metadata, not the security replay guard. Signed MQTT nonce replay protection is enforced downstream by `consumer` and `online-receiver` with Redis `SET NX EX`.
- **Input validation** — `Topic::new()` enforces a 255-byte length cap per segment, requires `device_id` to be a valid non-nil UUID, and whitelists `feature_name` against known sensor types. UUIDs in the MQTT payload (`device_uuid`, `feature_uuid`) are validated and nil-UUID values are rejected. `AMQP_QUEUE_NAME` is checked at startup for length (1–255) and RabbitMQ-safe characters.
- **Payload size limit** — `get_bytes_from_payload()` rejects payloads larger than 65 536 bytes before any processing, preventing memory pressure from malicious publishers.
- **TLS hardening** — `.enable_server_cert_auth(true)` is set on `SslOptionsBuilder` to require broker certificate verification. TLS certificate paths are resolved to absolute paths via `env::current_dir()` and guarded against `..` path-traversal components.
- **Sensitive data removed from logs** — `debug!` calls that logged the full raw MQTT payload (containing `apiToken`) and the full `payload_str` were removed.
- **No secrets in Docker images** — The `.env_template` file is no longer copied into production images; secrets must be injected at runtime via Kubernetes Secrets or environment variables.
- **Startup secret validation** — `AMQP_URI` and `AMQP_HMAC_SECRET` must be non-empty; `MQTT_PASSWORD` must be non-empty when `MQTT_AUTH` is enabled. The process exits cleanly (without exposing credentials in the panic message) on validation failure.
- **No credentials in process termination messages** — Startup and reconnect `panic!` calls that could expose URI or credentials in lapin error messages were replaced with logged errors followed by clean process exit.

### Idiomatic Rust issues

- **Error propagation** — `async fn main()` returns `anyhow::Result<()>`, eliminating `std::process::exit` calls from async context; startup failures use `.context(...)?` and reconnect timeouts use `anyhow::bail!`. `MqttOptions::new()` and `AmqpClient::connect()` return `Result` instead of panicking internally. `MqttClient::connect()` returns `Result<(), paho_mqtt::Error>`; the retry loop was moved to `main` so callers control the retry policy.
- **Option/Result combinators** — Replaced `match`/`unwrap_or_else` chains with `?`, `.inspect_err(...).ok()?`, and `.ok_or_else()` / `.map_err()` throughout `main.rs`, `mqtt/mod.rs`, and `amqp/mod.rs`. `get_msg_byte()`, `message_payload_to_bytes()`, `get_bytes_from_payload()`, and `get_string_payload()` all return typed `Option`/`Result` instead of using empty-`vec![]` or empty-string sentinels.
- **Type safety improvements** — Replaced multiple boolean flags in `AmqpClient::is_initialized` with an explicit `InitLevel` enum (`Disconnected` → `Connection` → `Channel` → `Queue` → `Consumer`). `consumer_tag` changed from `ShortString` (empty-string sentinel) to `Option<ShortString>`. `Online` payload type corrected from `i64` to `bool`. `subscribe()` made generic over `S: AsRef<str>`.
- **Serde correctness** — Removed unused `Serialize` from `Notification<T>` (receive-only) and unused `Deserialize` from `Message<T>` (send-only). `Message::new()` made private. Added `deserialize_finite_f64` serde helper that rejects `NaN` and `±Infinity` for float sensor fields; `f64` sensor structs derive `PartialEq`, integer/bool types additionally derive `Eq`. Added `PartialEq`/`Eq` derives to `Topic` and `PartialEq` to `Notification<T>`.
- **Error display** — `AmqpError` variants display their inner context string via `#[error("{0}")]` with descriptive prefixes. Removed the dead `PublishMessageError` variant from `MessageError`.
- **Code hygiene** — Removed `#![allow(clippy::uninlined_format_args)]` from crate roots; all format arguments now use inline syntax (`{var}`). Replaced string concatenation with `format!()`. Replaced `unwrap().unwrap()` chains with `.expect("…")` carrying descriptive messages. Removed the unused `ca_files_path` field from `MqttConfig`. Replaced `env::var("ENV") != Ok("testing".to_string())` with `.as_deref()` to avoid a needless heap allocation. Fixed `clippy::pedantic` lints: redundant closures replaced with method references, raw string hash fixes.

### Chores

- **Dependency versions refreshed** — `lapin`, `tokio`, `paho-mqtt`, `tracing-appender`, `uuid`, `zeroize`, `hmac`, `sha2`, and `hex` were updated or normalized in `Cargo.toml`.

### Tests

- **Integration-test MQTT credentials externalized** — `receive_message_via_mqtt` now reads `MQTT_PUBLISH_USER` and `MQTT_PUBLISH_PASSWORD`, with local defaults for the Mosquitto ACL test user, instead of hardcoding the old `mosquser` credentials.
- **Integration-test client ids isolated** — MQTT integration tests suffix the configured client id with a fresh UUID to avoid persistent-session collisions between test runs.
- **Assertions aligned with early validation** — `wrong_get_msg_byte_unknown_sensor` and `wrong_sensor_type_for_process_mqtt_message` updated to assert that `Topic::new()` returns `Err` for unknown sensor types, replacing assertions that relied on the now-unreachable downstream path.
- **Test robustness** — Replaced `unwrap().unwrap()` chains with `.expect("…")` in integration tests. Removed unused `MessageError` import. Updated tests for cascading API changes (`connect()`, `MqttOptions::new()`, `get_msg_byte()`).
