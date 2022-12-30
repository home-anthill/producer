use dotenvy::dotenv;
use futures::{executor::block_on, stream::StreamExt};
use log::{debug, error, info, warn};
use paho_mqtt as mqtt;
use paho_mqtt::{ConnectOptions, SslOptions};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::string::ToString;
use std::{env, fs};
use std::{process, time::Duration};

use producer::amqp::AmqpClient;
use producer::models::get_msq_byte;
use producer::models::topic::Topic;

const TOPICS: &[&str] = &[
    "sensors/+/temperature",
    "sensors/+/humidity",
    "sensors/+/light",
    "sensors/+/motion",
    "sensors/+/airquality",
    "sensors/+/airpressure",
];
const QOS: i32 = 0;
const COMBINED_CA_FILES_PATH: &str = "./rootca_and_cert.pem";

#[tokio::main]
async fn main() {
    // 1. Init logger
    log4rs::init_file("log4rs.yaml", Default::default()).unwrap();
    info!(target: "app", "Starting application...");

    // 2. Load the .env file
    dotenv().ok();

    // 3. Print .env vars
    let amqp_uri = env::var("AMQP_URI").expect("AMQP_URI is not found.");
    let amqp_queue_name = env::var("AMQP_QUEUE_NAME").expect("AMQP_QUEUE_NAME is not found.");
    let mqtt_url = env::var("MQTT_URL").expect("MQTT_URL is not found.");
    let mqtt_port = env::var("MQTT_PORT").expect("MQTT_PORT is not found.");
    let mqtt_client_id = env::var("MQTT_CLIENT_ID").expect("MQTT_CLIENT_ID is not found.");
    let mqtt_auth = env::var("MQTT_AUTH").expect("MQTT_AUTH is not found.");
    let mqtt_user = env::var("MQTT_USER").expect("MQTT_USER is not found.");
    let mqtt_password = env::var("MQTT_PASSWORD").expect("MQTT_PASSWORD is not found.");
    let mqtt_tls = env::var("MQTT_TLS").expect("MQTT_TLS is not found.");
    let root_ca = env::var("ROOT_CA").expect("ROOT_CA is not found.");
    let mqtt_cert_file = env::var("MQTT_CERT_FILE").expect("MQTT_CERT_FILE is not found.");
    let mqtt_key_file = env::var("MQTT_KEY_FILE").expect("MQTT_KEY_FILE is not found.");
    info!(target: "app", "AMQP_URI = {}", amqp_uri);
    info!(target: "app", "AMQP_QUEUE_NAME = {}", amqp_queue_name);
    info!(target: "app", "MQTT_URL = {}", mqtt_url);
    info!(target: "app", "MQTT_PORT = {}", mqtt_port);
    info!(target: "app", "MQTT_CLIENT_ID = {}", mqtt_client_id);
    info!(target: "app", "MQTT_AUTH = {}", mqtt_auth);
    info!(target: "app", "MQTT_USER = {}", mqtt_user);
    info!(target: "app", "MQTT_PASSWORD = {}", mqtt_password);
    info!(target: "app", "MQTT_TLS = {}", mqtt_tls);
    info!(target: "app", "ROOT_CA = {}", root_ca);
    info!(target: "app", "MQTT_CERT_FILE = {}", mqtt_cert_file);
    info!(target: "app", "MQTT_KEY_FILE = {}", mqtt_key_file);

    // 4. Init RabbitMQ
    info!(target: "app", "Initializing RabbitMQ");
    let mut amqp_client: AmqpClient = AmqpClient::new(amqp_uri.clone(), amqp_queue_name.clone());
    amqp_client.connect_with_retry_loop().await;

    // 5. Create CA file in 'COMBINED_CA_FILES_PATH'
    // merging 'root_ca' and 'mqtt_cert_file',
    // otherwise, paho.mqtt.rust won't be able to connect.
    if mqtt_tls == "true" {
        info!(target: "app", "Preparing MQTT CA file");
        merge_ca_files(&root_ca, &mqtt_cert_file);
    }

    // 6. Init MQTT
    let mqtt_uri = if mqtt_tls == "true" {
        format!("ssl://{}:{}", mqtt_url, mqtt_port)
    } else {
        format!("tcp://{}:{}", mqtt_url, mqtt_port)
    };
    info!(target: "app", "mqtt_uri = {}", mqtt_uri);

    // Create the client. Use an unique ID for a persistent session
    let create_opts = mqtt::CreateOptionsBuilder::new()
        .server_uri(mqtt_uri)
        .client_id(mqtt_client_id)
        .finalize();
    let mut mqtt_client = mqtt::AsyncClient::new(create_opts).unwrap_or_else(|err| {
        error!(target: "app", "Error creating MQTT client: {:?}", err);
        process::exit(1);
    });
    // Get message stream before connecting.
    let mut strm = mqtt_client.get_stream(25);

    info!(target: "app", "Creating MQTT ConnectOptions...");
    let conn_opts = build_mqtt_connect_options(
        &mqtt_auth,
        &mqtt_user,
        &mqtt_password,
        &mqtt_tls,
        &mqtt_cert_file,
        &mqtt_key_file,
    );

    // 7. Make the connection to the broker
    info!(target: "app", "Connecting to the MQTT server with ConnectOptions...");
    while let Err(err) = mqtt_client.connect(conn_opts.clone()).await {
        error!(target: "app", "MQTT Connection error, retying in 10 seconds. Error = {:?}", err);
        tokio::time::sleep(Duration::from_millis(30000)).await;
    }
    info!(target: "app", "MQTT Connection succeeded");

    // 8. Subscribe to the topics
    info!(target: "app", "Subscribing to the topics...");
    let topic_result = subscribe_topics(&mqtt_client).await;
    match topic_result {
        Ok(_) => info!(target: "app", "Subscription to the topics completed"),
        Err(err) => {
            error!(target: "app", "Cannot subscribe to topics. Error = {:?}", err);
            process::exit(1);
        }
    }

    // 9. Wait for incoming messages
    info!(target: "app", "Waiting for incoming MQTT messages");
    while let Some(msg_opt) = strm.next().await {
        if let Some(msg) = msg_opt {
            debug!(target: "app", "MQTT message received");
            let topic: Topic = Topic::new(msg.topic());
            debug!(target: "app", "MQTT message topic = {}", &topic);
            let payload_str: &str = match std::str::from_utf8(msg.payload()) {
                Ok(res) => {
                    debug!(target: "app", "MQTT utf8 payload_str: {}", res);
                    res
                }
                Err(err) => {
                    error!(target: "app", "Cannot read MQTT message payload as utf8. Error = {:?}", err);
                    ""
                }
            };
            let msg_byte: Vec<u8> = get_msq_byte(&topic, payload_str);
            if msg_byte.is_empty() {
                debug!(target: "app", "Empty msg_byte received");
                continue;
            }
            // if msg_byte is not empty
            block_on(async {
                if !amqp_client.is_connected() {
                    debug!(target: "app", "AMQP channel is not connected, reconnecting...");
                    amqp_client.connect_with_retry_loop().await;
                }
                debug!(target: "app", "Publishing message via AMQP...");
                // send via AMQP
                match amqp_client.publish_message(msg_byte).await {
                    Ok(_) => {
                        debug!(target: "app", "AMQP message published to queue {}", amqp_queue_name);
                    }
                    Err(err) => {
                        error!(target: "app", "Cannot publish AMQP message to queue {}. Err ={:?}", amqp_queue_name, err);
                    }
                };
            });
        } else {
            // msg_opt="None" means we were disconnected. Try to reconnect...
            warn!(target: "app", "Lost connection. Attempting reconnect in 5 seconds...");
            while let Err(err) = mqtt_client.reconnect().await {
                error!(target: "app", "Error reconnecting: {:?}, retrying in 5 seconds...", err);
                tokio::time::sleep(Duration::from_millis(5000)).await;
            }
        }
    }
}

async fn subscribe_topics(
    cli: &mqtt::AsyncClient,
) -> Result<paho_mqtt::ServerResponse, paho_mqtt::Error> {
    let topics: Vec<String> = TOPICS.iter().map(|s| s.to_string()).collect();
    info!(target: "app", "Subscribing to MQTT topics: {:?}", topics);
    let qos = vec![QOS; topics.len()];
    // We subscribe to the topic(s) we want here.
    cli.subscribe_many(&topics, &qos).await
}

fn build_mqtt_connect_options(
    mqtt_auth: &String,
    mqtt_user: &String,
    mqtt_password: &String,
    mqtt_tls: &String,
    mqtt_cert_file: &String,
    mqtt_key_file: &String,
) -> ConnectOptions {
    // Define the set of options for the connection
    let lwt = mqtt::Message::new("test", "Subscriber lost connection", 1);
    let mut conn_opts: ConnectOptions = ConnectOptions::new();
    let mut new_con_builder = mqtt::ConnectOptionsBuilder::new();
    let connect_options_builder = new_con_builder
        .keep_alive_interval(Duration::from_secs(20))
        .mqtt_version(mqtt::MQTT_VERSION_3_1_1)
        // Using a "persistent" (non-clean) session
        // so the broker keeps subscriptions and messages through reconnects
        .clean_session(false)
        .will_message(lwt);

    if mqtt_auth == "true" {
        info!(target: "app", "build_mqtt_connect_options - MQTT AUTH is enabled, setting username and password");
        connect_options_builder
            .user_name(mqtt_user)
            .password(mqtt_password);
    }

    if mqtt_tls == "true" {
        info!(target: "app", "build_mqtt_connect_options - MQTT TLS is enabled, creating ConnectOptions with certificates");
        let ssl_options = build_ssl_options(mqtt_cert_file, mqtt_key_file);
        if let Some(ssl_opt) = ssl_options {
            conn_opts = connect_options_builder.ssl_options(ssl_opt).finalize();
            info!(target: "app", "build_mqtt_connect_options - MQTT ConnectOptions with SSL created successfully");
        } else {
            error!(target: "app", "build_mqtt_connect_options - Cannot create MQTT ConnectOptions with certificates.");
            process::exit(1);
        }
    } else {
        conn_opts = connect_options_builder.finalize();
        info!(target: "app", "build_mqtt_connect_options - MQTT ConnectOptions created WITHOUT SSL");
    };
    conn_opts
}

fn merge_ca_files(root_ca: &String, mqtt_cert_file: &String) {
    // re-create a new file appending two certificates:
    // - ROOT_CA file (ISRG_Root_X1.pem in case of Let's Encrypt)
    // - MQTT_CERT_FILE file (cert.pem in case of Let's Encrypt)
    if Path::new(COMBINED_CA_FILES_PATH).exists() {
        fs::remove_file(COMBINED_CA_FILES_PATH)
            .expect("Cannot remove existing COMBINED_CA_FILES_PATH");
    }
    let combined_root_ca_res = File::create(COMBINED_CA_FILES_PATH);
    match &combined_root_ca_res {
        Ok(_res) => {
            info!(target: "app", "merge_ca_files - {} file created", COMBINED_CA_FILES_PATH);
        }
        Err(err) => {
            error!(target: "app", "merge_ca_files - cannot create {} file, err = {:?}", COMBINED_CA_FILES_PATH, err);
        }
    }
    let mut combined_root_ca = combined_root_ca_res.unwrap();
    let root_ca_vec = fs::read(&root_ca).expect("Cannot read root_ca_vec as byte array");
    let mqtt_cert_file_vec =
        fs::read(&mqtt_cert_file).expect("Cannot read mqtt_cert_file as byte array");
    combined_root_ca
        .write_all(&root_ca_vec)
        .expect("Cannot write root_ca_vec to COMBINED_CA_FILES_PATH");
    combined_root_ca
        .write_all(b"\n")
        .expect("Cannot write new line to COMBINED_CA_FILES_PATH");
    combined_root_ca
        .write_all(&mqtt_cert_file_vec)
        .expect("Cannot write mqtt_cert_file_vec to COMBINED_CA_FILES_PATH");
}

fn build_ssl_options(mqtt_cert_file: &String, mqtt_key_file: &String) -> Option<SslOptions> {
    // I need COMBINED_CA_FILES_PATH, check above at step 5.
    let mut trust_store = env::current_dir().expect("Cannot get current dir for trust_store.");
    trust_store.push(COMBINED_CA_FILES_PATH);
    let mut key_store = env::current_dir().expect("Cannot get current dir for key_store.");
    key_store.push(mqtt_cert_file);
    let mut private_key = env::current_dir().expect("Cannot get current dir for private_key.");
    private_key.push(mqtt_key_file);
    if !trust_store.exists() {
        error!(target: "app", "get_ssl_options - trust_store file does not exist: {:?}", trust_store);
        return None;
    }
    if !key_store.exists() {
        error!(target: "app", "get_ssl_options - key_store file does not exist: {:?}", key_store);
        return None;
    }
    if !private_key.exists() {
        error!(target: "app", "get_ssl_options - private_key file does not exist: {:?}", private_key);
        return None;
    }

    debug!(target: "app", "get_ssl_options - trust_store {:?}", trust_store);
    debug!(target: "app", "get_ssl_options - key_store {:?}", key_store);
    debug!(target: "app", "get_ssl_options - private_key {:?}", private_key);

    let ssl_opts = mqtt::SslOptionsBuilder::new()
        .trust_store(trust_store)
        .unwrap()
        .key_store(key_store)
        .unwrap()
        .private_key(private_key)
        .unwrap()
        .finalize();
    Some(ssl_opts)
}
