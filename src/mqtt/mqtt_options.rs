use std::fs::{File, read};
use std::io::Write;
use std::{env, time::Duration};

use hmac::{Hmac, KeyInit, Mac};
use paho_mqtt::{
    ConnectOptions, ConnectOptionsBuilder, CreateOptions, CreateOptionsBuilder, Message, SslOptions, SslOptionsBuilder,
};
use sha2::Sha256;
use tracing::{debug, error, info, warn};

use crate::errors::mqtt_error::MqttError;
use crate::mqtt::COMBINED_CA_FILES_PATH;
use crate::mqtt::mqtt_config::MqttConfig;

type HmacSha256 = Hmac<Sha256>;

/// Builds the LWT payload as a JSON object containing the status string and
/// an HMAC-SHA256 signature so consumers can authenticate the will message.
fn build_lwt_payload(hmac_secret: &[u8]) -> String {
    const STATUS: &[u8] = b"lost connection";
    let mut mac = HmacSha256::new_from_slice(hmac_secret).expect("HMAC accepts any key length");
    mac.update(STATUS);
    let sig = hex::encode(mac.finalize().into_bytes());
    format!(r#"{{"status":"lost connection","hmac-sha256":"{sig}"}}"#)
}

pub struct MqttOptions {
    pub create_opts: CreateOptions,
    pub conn_opts: ConnectOptions,
}

impl MqttOptions {
    pub fn new(mqtt_config: &MqttConfig) -> Result<Self, anyhow::Error> {
        let mqtt_uri = if mqtt_config.tls {
            format!("ssl://{}:{}", mqtt_config.url, mqtt_config.port)
        } else {
            format!("tcp://{}:{}", mqtt_config.url, mqtt_config.port)
        };
        info!(target: "app", "mqtt_uri = {}", &mqtt_uri);

        // Create CA file in 'COMBINED_CA_FILES_PATH' merging 'root_ca' and 'mqtt_cert_file',
        // otherwise, paho.mqtt.rust won't be able to connect.
        if mqtt_config.tls {
            info!(target: "app", "Preparing MQTT CA file");
            Self::merge_ca_files(&mqtt_config.root_ca_file, &mqtt_config.cert_file)?;
        }

        let create_options = CreateOptionsBuilder::new()
            .server_uri(mqtt_uri)
            .client_id(&mqtt_config.client_id)
            .finalize();

        info!(target: "app", "Creating MQTT ConnectOptions...");
        let conn_opts = Self::build_connect_options(mqtt_config)?;

        Ok(Self {
            create_opts: create_options,
            conn_opts,
        })
    }

    fn merge_ca_files(root_ca: &str, mqtt_cert_file: &str) -> Result<(), anyhow::Error> {
        // Re-create a new file appending two certificates:
        // - ROOT_CA file (ISRG_Root_X1.pem in case of Let's Encrypt)
        // - MQTT_CERT_FILE file (cert.pem in case of Let's Encrypt)
        // File::create truncates if the file already exists, no need to remove first.
        // Use restrictive permissions (0600) to prevent other users from reading the certs.
        // Resolve to an absolute path so the file is written to a predictable location
        // regardless of the process working directory.
        use std::os::unix::fs::OpenOptionsExt;
        let combined_path = env::current_dir()?.join(COMBINED_CA_FILES_PATH);
        let mut combined_root_ca = File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&combined_path)?;
        debug!(target: "app", "merge_ca_files - {:?} file created", combined_path);
        let root_ca_vec = read(root_ca)?;
        let mqtt_cert_file_vec = read(mqtt_cert_file)?;
        combined_root_ca.write_all(&root_ca_vec)?;
        combined_root_ca.write_all(b"\n")?;
        combined_root_ca.write_all(&mqtt_cert_file_vec)?;
        Ok(())
    }

    fn build_connect_options(config: &MqttConfig) -> Result<ConnectOptions, anyhow::Error> {
        let lwt_topic = format!("clients/{}/status", config.client_id);
        let lwt_payload = build_lwt_payload(config.hmac_secret.as_bytes());
        let lwt = Message::new(lwt_topic, lwt_payload, 1);
        let mut new_con_builder = ConnectOptionsBuilder::new();
        let connect_options_builder = new_con_builder
            .keep_alive_interval(Duration::from_secs(20))
            // Using a "persistent" (non-clean) session
            // so the broker keeps subscriptions and messages through reconnects
            .clean_session(false)
            .will_message(lwt);

        if config.auth {
            warn!(target: "app", "build_connect_options - MQTT authentication is enabled, setting username and password");
            connect_options_builder
                .user_name(config.user.as_str())
                .password(config.password.as_str());
        }

        if config.tls {
            warn!(target: "app", "build_connect_options - MQTT TLS is enabled, creating ConnectOptions with certificates");
            match Self::build_ssl_options(&config.cert_file, &config.key_file, COMBINED_CA_FILES_PATH) {
                Ok(ssl_options) => {
                    debug!(target: "app", "build_connect_options - MQTT ConnectOptions with SSL created successfully");
                    connect_options_builder.ssl_options(ssl_options);
                }
                Err(err) => {
                    error!(target: "app", "build_connect_options - Cannot create MQTT ConnectOptions with certificates, err = {:?}", err);
                    return Err(err);
                }
            }
        }
        Ok(connect_options_builder.finalize())
    }

    fn build_ssl_options(
        mqtt_cert_file: &str,
        mqtt_key_file: &str,
        combined_ca_files_path: &str,
    ) -> Result<SslOptions, anyhow::Error> {
        let cwd = env::current_dir()?;
        let trust_store = cwd.join(combined_ca_files_path);
        let key_store = cwd.join(mqtt_cert_file);
        let private_key = cwd.join(mqtt_key_file);

        // Reject any path containing `..` to prevent directory traversal.
        for path in [&key_store, &private_key] {
            if path.components().any(|c| c == std::path::Component::ParentDir) {
                return Err(anyhow::anyhow!(
                    "certificate path must not contain '..' components: {:?}",
                    path
                ));
            }
        }

        if !trust_store.exists() {
            error!(target: "app", "build_ssl_options - trust_store file does not exist: {:?}", trust_store);
            return Err(anyhow::Error::from(MqttError::FileNotFound("trust_store".to_string())));
        }
        if !key_store.exists() {
            error!(target: "app", "build_ssl_options - key_store file does not exist: {:?}", key_store);
            return Err(anyhow::Error::from(MqttError::FileNotFound("key_store".to_string())));
        }
        if !private_key.exists() {
            error!(target: "app", "build_ssl_options - private_key file does not exist: {:?}", private_key);
            return Err(anyhow::Error::from(MqttError::FileNotFound("private_key".to_string())));
        }

        debug!(target: "app", "build_ssl_options - trust_store {:?}", trust_store);
        debug!(target: "app", "build_ssl_options - key_store {:?}", key_store);
        debug!(target: "app", "build_ssl_options - private_key {:?}", private_key);

        let ssl_opts = SslOptionsBuilder::new()
            .enable_server_cert_auth(true)
            .trust_store(trust_store)?
            .key_store(key_store)?
            .private_key(private_key)?
            .finalize();
        Ok(ssl_opts)
    }
}

#[cfg(test)]
mod tests {
    use crate::mqtt::mqtt_options::{MqttOptions, build_lwt_payload};
    use serde_json::Value;

    #[test]
    fn build_lwt_payload_contains_status_and_signature() {
        let payload = build_lwt_payload(b"secret");
        let value: Value = serde_json::from_str(&payload).expect("LWT payload should be valid JSON");

        assert_eq!(value["status"], "lost connection");
        assert_eq!(
            value["hmac-sha256"]
                .as_str()
                .expect("signature should be a string")
                .len(),
            64
        );
    }

    #[test]
    fn wrong_build_ssl_options_missing_trust_store() {
        let err = MqttOptions::build_ssl_options("cert.pem", "key.pem", "missing-ca.pem")
            .expect_err("missing trust store should fail");

        assert!(err.to_string().contains("trust_store"));
    }

    #[test]
    fn wrong_build_ssl_options_rejects_parent_dir_cert_path() {
        let err = MqttOptions::build_ssl_options("../cert.pem", "key.pem", "ca.pem")
            .expect_err("parent directory in certificate path should fail");

        assert!(err.to_string().contains("must not contain '..'"));
    }
}
