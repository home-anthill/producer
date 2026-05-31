use std::env;
use std::fmt;

use dotenvy::dotenv;
use serde::Deserialize;
use tracing::info;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use zeroize::Zeroizing;

#[derive(Deserialize)]
pub struct Env {
    pub amqp_uri: Zeroizing<String>,
    pub amqp_hmac_secret: Zeroizing<String>,
    pub amqp_queue_name: String,
    pub mqtt_url: String,
    pub mqtt_port: u16,
    pub mqtt_client_id: String,
    pub mqtt_auth: bool,
    pub mqtt_user: Zeroizing<String>,
    pub mqtt_password: Zeroizing<String>,
    pub mqtt_tls: bool,
    pub root_ca: String,
    pub mqtt_cert_file: String,
    pub mqtt_key_file: String,
}

impl fmt::Debug for Env {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Env")
            .field("amqp_uri", &"[REDACTED]")
            .field("amqp_hmac_secret", &"[REDACTED]")
            .field("amqp_queue_name", &self.amqp_queue_name)
            .field("mqtt_url", &self.mqtt_url)
            .field("mqtt_port", &self.mqtt_port)
            .field("mqtt_client_id", &self.mqtt_client_id)
            .field("mqtt_auth", &self.mqtt_auth)
            .field("mqtt_user", &"[REDACTED]")
            .field("mqtt_password", &"[REDACTED]")
            .field("mqtt_tls", &self.mqtt_tls)
            .field("root_ca", &self.root_ca)
            .field("mqtt_cert_file", &self.mqtt_cert_file)
            .field("mqtt_key_file", &self.mqtt_key_file)
            .finish()
    }
}

pub fn init() -> Env {
    // Load the .env file
    dotenv().ok();
    let env = envy::from_env::<Env>().expect("failed to parse environment variables");

    // Validate required secret fields are non-empty
    if env.amqp_uri.is_empty() {
        eprintln!("FATAL: AMQP_URI must not be empty");
        std::process::exit(1);
    }
    if env.amqp_hmac_secret.is_empty() {
        eprintln!("FATAL: AMQP_HMAC_SECRET must not be empty");
        std::process::exit(1);
    }
    if env.mqtt_auth && env.mqtt_password.is_empty() {
        eprintln!("FATAL: MQTT_PASSWORD must not be empty when MQTT_AUTH is enabled");
        std::process::exit(1);
    }

    // Validate AMQP queue name: RabbitMQ allows 1–255 bytes, ASCII alphanumeric + _ - . : @
    let queue_name_ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '@');
    if env.amqp_queue_name.is_empty() || env.amqp_queue_name.len() > 255 {
        eprintln!("FATAL: AMQP_QUEUE_NAME must be 1–255 characters");
        std::process::exit(1);
    }
    if !env.amqp_queue_name.chars().all(queue_name_ok) {
        eprintln!("FATAL: AMQP_QUEUE_NAME contains invalid characters (allowed: a-z A-Z 0-9 _ - . : @)");
        std::process::exit(1);
    }

    // Configure logging if not in test env
    if env::var("ENV").as_deref() != Ok("testing") {
        let stdout = std::io::stdout.with_filter(|meta| meta.target() == "app");
        let info_file = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("info")
            .filename_suffix("log")
            .max_log_files(5)
            .build("./logs")
            .expect("initializing rolling info_file appender failed")
            .with_max_level(tracing::Level::INFO);
        let error_file = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("error")
            .filename_suffix("log")
            .max_log_files(5)
            .build("./logs")
            .expect("initializing rolling error_file appender failed")
            .with_filter(|meta| meta.target() == "app")
            .with_max_level(tracing::Level::ERROR);
        let writer = info_file.and(error_file).and(stdout);
        let _ = tracing_subscriber::fmt()
            .compact()
            .with_writer(writer)
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .try_init();
    }

    info!(target: "app", "Starting application...");

    // Print .env vars
    print_env(&env);
    env
}

fn print_env(env: &Env) {
    info!(target: "app", "amqp_uri = [REDACTED]");
    info!(target: "app", "amqp_hmac_secret = [REDACTED]");
    info!(target: "app", "amqp_queue_name = {}", env.amqp_queue_name);
    info!(target: "app", "mqtt_url = {}", env.mqtt_url);
    info!(target: "app", "mqtt_port = {}", env.mqtt_port);
    info!(target: "app", "mqtt_client_id = {}", env.mqtt_client_id);
    info!(target: "app", "mqtt_auth = {}", env.mqtt_auth);
    info!(target: "app", "mqtt_user = [REDACTED]");
    info!(target: "app", "mqtt_password = [REDACTED]");
    info!(target: "app", "mqtt_tls = {}", env.mqtt_tls);
    info!(target: "app", "root_ca = {}", env.root_ca);
    info!(target: "app", "mqtt_cert_file = {}", env.mqtt_cert_file);
    info!(target: "app", "mqtt_key_file = {}", env.mqtt_key_file);
}
