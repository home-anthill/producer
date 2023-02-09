use log::{debug, error, info};
use std::string::String;
use std::time::Duration;

use lapin::publisher_confirm::PublisherConfirm;
use lapin::{
    options::{BasicPublishOptions, QueueDeclareOptions},
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties, Queue,
};
use thiserror::Error;

// custom error, based on 'thiserror' library
#[derive(Error, Debug)]
pub enum AmqpError {
    #[error("amqp_client publish error")]
    Publish(lapin::Error),
    #[error("amqp_client not initialized error")]
    Uninitialized(String),
    #[error("amqp_client is reconnecting error")]
    Connecting(String),
}

pub struct AmqpClient {
    connecting: bool,
    connection: Option<Connection>,
    channel: Option<Channel>,
    queue: Option<Queue>,
    pub amqp_uri: String,
    pub amqp_queue_name: String,
}

impl AmqpClient {
    pub fn new(amqp_uri: String, amqp_queue_name: String) -> Self {
        Self {
            connecting: false,
            connection: None,
            channel: None,
            queue: None,
            amqp_uri,
            amqp_queue_name,
        }
    }

    // init or re-init the amqp client trying to connect in a loop until success
    pub async fn connect_with_retry_loop(&mut self) {
        info!(target: "app", "connect_with_retry_loop - trying to connect to amqp_uri={} with queue={}", &self.amqp_uri, &self.amqp_queue_name);
        self.connecting = true;
        self.create_connection().await;
        self.create_channel().await.unwrap();
        self.declare_queue().await.unwrap();
        self.connecting = false;
        info!(target: "app", "connect_with_retry_loop - AMQP connection done!");
    }

    // before calling this method you must be sure that is_connected() returns true
    pub async fn publish_message(&self, msg_byte: Vec<u8>) -> Result<PublisherConfirm, AmqpError> {
        debug!(target: "app", "publish_message - publishing byte message to queue {}", &self.amqp_queue_name);
        if self.connecting {
            error!(target: "app", "publish_message - cannot publish while connecting");
            return Err(AmqpError::Connecting(String::from(
                "cannot publish while connecting",
            )));
        }
        let publish_result: lapin::Result<PublisherConfirm> = self
            .channel
            .as_ref()
            .unwrap()
            .basic_publish(
                "",
                &self.amqp_queue_name,
                BasicPublishOptions::default(),
                msg_byte.as_slice(),
                BasicProperties::default(),
            )
            .await;
        match publish_result {
            Ok(confirm) => Ok(confirm),
            Err(err) => Err(AmqpError::Publish(err)),
        }
    }

    pub fn is_connected(&self) -> bool {
        // check if you are calling this method on an initialized amqp_client instance
        // (with both connection, channel and queue)
        let init_result: Result<(), AmqpError> = self.is_initialized(true, true, true);
        if init_result.is_err() {
            return false;
        }
        self.connection.as_ref().unwrap().status().connected()
            && self.channel.as_ref().unwrap().status().connected()
    }

    async fn create_connection(&mut self) {
        info!(target: "app", "create_connection - creating AMQP connection...");
        self.connection = loop {
            let options = ConnectionProperties::default()
                .with_executor(tokio_executor_trait::Tokio::current())
                .with_reactor(tokio_reactor_trait::Tokio);
            match Connection::connect(&self.amqp_uri, options).await {
                Ok(connection) => {
                    info!(target: "app", "create_connection - AMQP connection established");
                    break Some(connection);
                }
                Err(err) => {
                    error!(target: "app", "create_connection - cannot create AMQP connection, retrying in 10 seconds. Err = {:?}", err);
                    tokio::time::sleep(Duration::from_millis(10000)).await;
                }
            };
        };
        self.connection.as_ref().unwrap().on_error(|err| {
            error!(target: "app", "create_connection - AMQP connection error = {:?}", err);
        });
    }

    // private method that must be called after create_connection()
    async fn create_channel(&mut self) -> Result<(), AmqpError> {
        info!(target: "app", "create_channel - creating AMQP channel...");
        // check if you are calling this method on an initialized amqp_client instance (with ONLY connection)
        let init_result: Result<(), AmqpError> = self.is_initialized(true, false, false);
        if init_result.is_err() {
            return Err(init_result.unwrap_err());
        }
        self.channel = loop {
            match self.connection.as_ref().unwrap().create_channel().await {
                Ok(channel) => {
                    info!(target: "app", "create_channel - AMQP channel created");
                    break Some(channel);
                }
                Err(err) => {
                    error!(target: "app", "create_channel - cannot create AMQP channel, retrying in 10 seconds. Err = {:?}", err);
                    tokio::time::sleep(Duration::from_millis(10000)).await;
                }
            };
        };
        Ok(())
    }

    // private method that must be called after both create_connection() and create_channel()
    async fn declare_queue(&mut self) -> Result<(), AmqpError> {
        info!(target: "app", "declare_queue - creating AMQP queue...");
        // check if you are calling this method on an initialized amqp_client instance
        // (with both connection and channel, but not queue)
        let init_result: Result<(), AmqpError> = self.is_initialized(true, true, false);
        if init_result.is_err() {
            return Err(init_result.unwrap_err());
        }
        self.queue = loop {
            match self
                .channel
                .as_ref()
                .unwrap()
                .queue_declare(
                    &self.amqp_queue_name,
                    QueueDeclareOptions::default(),
                    FieldTable::default(),
                )
                .await
            {
                Ok(channel) => {
                    info!(target: "app", "declare_queue - AMQP queue created");
                    break Some(channel);
                }
                Err(err) => {
                    error!(target: "app", "declare_queue - cannot create AMQP queue, retrying in 10 seconds. Err = {:?}", err);
                    tokio::time::sleep(Duration::from_millis(10000)).await;
                }
            };
        };
        Ok(())
    }

    fn is_initialized(
        &self,
        check_connection: bool,
        check_channel: bool,
        check_queue: bool,
    ) -> Result<(), AmqpError> {
        if check_connection && self.connection.is_none() {
            error!(target: "app", "is_initialized - amqp_client connection not initialized. You must call AmqpClient::new()");
            return Err(AmqpError::Uninitialized(String::from(
                "amqp_client connection not initialized. You must call AmqpClient::new()",
            )));
        }
        if check_channel && self.channel.is_none() {
            error!(target: "app", "is_initialized - amqp_client channel not initialized. You must call AmqpClient::new()");
            return Err(AmqpError::Uninitialized(String::from(
                "amqp_client channel not initialized. You must call AmqpClient::new()",
            )));
        }
        if check_queue && self.queue.is_none() {
            error!(target: "app", "is_initialized - amqp_client queue not initialized. You must call AmqpClient::new()");
            return Err(AmqpError::Uninitialized(String::from(
                "amqp_client queue not initialized. You must call AmqpClient::new()",
            )));
        }
        Ok(())
    }
}
