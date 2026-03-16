use std::env;

use dotenvy::dotenv;
use serde::Deserialize;
use tracing::info;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::writer::MakeWriterExt;

#[derive(Deserialize, Debug)]
pub struct Env {
    pub amqp_uri: String,
    pub amqp_queue_name: String,
    pub mqtt_url: String,
    pub mqtt_port: u16,
    pub mqtt_client_id: String,
    pub mqtt_auth: bool,
    pub mqtt_user: String,
    pub mqtt_password: String,
    pub mqtt_tls: bool,
    pub root_ca: String,
    pub mqtt_cert_file: String,
    pub mqtt_key_file: String,
}

pub fn init() -> Env {
    // Load the .env file
    dotenv().ok();
    let env = envy::from_env::<Env>().expect("Failed to load env vars");

    // Configure logging if not in test env
    if env::var("ENV") != Ok("testing".to_string()) {
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
        tracing_subscriber::fmt()
            .compact()
            .with_writer(writer)
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }

    info!(target: "app", "Starting application...");

    // Print .env vars
    print_env(&env);
    env
}

fn print_env(env: &Env) {
    info!(target: "app", "env = {:?}", env);
    info!(target: "app", "amqp_uri = {}", env.amqp_uri);
    info!(target: "app", "amqp_queue_name = {}", env.amqp_queue_name);
    info!(target: "app", "mqtt_url = {}", env.mqtt_url);
    info!(target: "app", "mqtt_port = {}", env.mqtt_port);
    info!(target: "app", "mqtt_client_id = {}", env.mqtt_client_id);
    info!(target: "app", "mqtt_auth = {}", env.mqtt_auth);
    info!(target: "app", "mqtt_user = {}", env.mqtt_user);
    info!(target: "app", "mqtt_password = {}", env.mqtt_password);
    info!(target: "app", "mqtt_tls = {}", env.mqtt_tls);
    info!(target: "app", "root_ca = {}", env.root_ca);
    info!(target: "app", "mqtt_cert_file = {}", env.mqtt_cert_file);
    info!(target: "app", "mqtt_key_file = {}", env.mqtt_key_file);
}
