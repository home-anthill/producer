use lapin::options::BasicConsumeOptions;
use lapin::types::ShortString;
use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, Consumer, Queue,
    options::{BasicPublishOptions, QueueDeclareOptions},
    types::FieldTable,
};
use tracing::{debug, error, info};

use crate::errors::amqp_error::AmqpError;

pub struct AmqpClient {
    amqp_uri: String,
    amqp_queue_name: ShortString,
    consumer_tag: ShortString,
    properties: ConnectionProperties,
    connection: Option<Connection>,
    channel: Option<Channel>,
    queue: Option<Queue>,
    pub consumer: Option<Consumer>,
    connecting: bool,
}

impl AmqpClient {
    pub fn new(amqp_uri: String, amqp_queue_name: String) -> Self {
        Self {
            amqp_uri,
            amqp_queue_name: amqp_queue_name.into(),
            properties: ConnectionProperties::default()
                .with_connection_name("amqp-client".into())
                .enable_auto_recover(),
            connection: None,
            channel: None,
            queue: None,
            connecting: false,
            consumer: None,
            consumer_tag: "".into(),
        }
    }

    // Use the builder pattern to init an optional param
    pub fn consumer(mut self, consumer_tag: String) -> AmqpClient {
        self.consumer_tag = consumer_tag.into();
        self
    }

    pub fn is_connected(&self, with_consumer: bool) -> bool {
        // check if you are calling this method on an initialized amqp_client instance
        // (with both connection, channel and queue)
        let init_result: Result<(), AmqpError> = self.is_initialized(true, true, true, with_consumer);
        if init_result.is_err() {
            return false;
        }
        self.connection.as_ref().unwrap().status().connected() && self.channel.as_ref().unwrap().status().connected()
    }

    pub async fn connect(&mut self, is_consumer: bool) {
        info!(target: "app", "connect - trying to connect to amqp_uri={} with queue={}", &self.amqp_uri, &self.amqp_queue_name);
        self.connecting = true;
        self.create_connection().await.expect("cannot create connection");
        info!(target: "app", "connect - creating channel...");
        self.create_channel().await.expect("cannot create channel");
        info!(target: "app", "connect - declaring queue...");
        self.declare_queue().await.expect("cannot declare queue");
        if is_consumer {
            info!(target: "app", "connect - creating consumer...");
            self.create_consumer().await.expect("cannot declare consumer");
        }
        self.connecting = false;
        info!(target: "app", "connect - AMQP connection done!");
    }

    async fn create_connection(&mut self) -> Result<(), AmqpError> {
        info!(target: "app", "create_connection - creating AMQP connection...");
        self.connection = match Connection::connect(&self.amqp_uri, self.properties.clone()).await {
            Ok(connection) => {
                info!(target: "app", "create_connection - AMQP connection established");
                Some(connection)
            }
            Err(err) => {
                error!(target: "app", "create_connection - cannot create AMQP connection. Err = {:?}", err);
                return Err(AmqpError::ConnectionError(String::from("amqp_client connection error")));
            }
        };
        Ok(())
    }

    // private method that must be called after create_connection()
    async fn create_channel(&mut self) -> Result<(), AmqpError> {
        info!(target: "app", "create_channel - creating AMQP channel...");
        self.is_initialized(true, false, false, false)?;
        self.channel = match self.connection.as_ref().unwrap().create_channel().await {
            Ok(channel) => {
                info!(target: "app", "create_channel - AMQP channel created");
                Some(channel)
            }
            Err(err) => {
                error!(target: "app", "create_channel - cannot create AMQP channel. Err = {:?}", err);
                return Err(AmqpError::ConnectionError("amqp_client channel creation error".into()));
            }
        };
        Ok(())
    }

    // private method that must be called after both create_connection() and create_channel()
    async fn declare_queue(&mut self) -> Result<(), AmqpError> {
        info!(target: "app", "declare_queue - creating AMQP queue...");
        self.is_initialized(true, true, false, false)?;
        self.queue = match self
            .channel
            .as_ref()
            .unwrap()
            .queue_declare(
                self.amqp_queue_name.clone(),
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
        {
            Ok(queue) => {
                info!(target: "app", "declare_queue - AMQP queue created");
                Some(queue)
            }
            Err(err) => {
                error!(target: "app", "declare_queue - cannot create AMQP queue. Err = {:?}", err);
                return Err(AmqpError::ConnectionError("amqp_client queue declaration error".into()));
            }
        };
        Ok(())
    }

    // private method that must be called after both create_connection(), create_channel() and create_queue()
    async fn create_consumer(&mut self) -> Result<(), AmqpError> {
        info!(target: "app", "create_consumer - creating AMQP consumer...");
        self.is_initialized(true, true, true, false)?;
        self.consumer = match self
            .channel
            .as_ref()
            .unwrap()
            .basic_consume(
                self.amqp_queue_name.clone(),
                self.consumer_tag.clone(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
        {
            Ok(consumer) => {
                info!(target: "app", "create_consumer - AMQP consumer created");
                Some(consumer)
            }
            Err(err) => {
                error!(target: "app", "create_consumer - cannot create AMQP consumer. Err = {:?}", err);
                return Err(AmqpError::ConnectionError("amqp_client consumer creation error".into()));
            }
        };
        Ok(())
    }

    // before calling this method you must be sure that a channel has been created
    pub async fn publish_message(&mut self, amqp_queue_name: &str, msg_byte: Vec<u8>) -> Result<(), AmqpError> {
        debug!(target: "app", "publish_message - publishing byte message to queue {}...", amqp_queue_name);
        if self.connecting {
            error!(target: "app", "publish_message - cannot publish while amqp_client is not initialized");
            return Err(AmqpError::Uninitialized(String::from(
                "cannot publish while amqp_client is not initialized",
            )));
        }
        let publish_result = self
            .channel
            .as_ref()
            .unwrap()
            .basic_publish(
                "".into(),
                amqp_queue_name.into(),
                BasicPublishOptions::default(),
                msg_byte.as_slice(),
                BasicProperties::default(),
            )
            .await;
        match publish_result {
            Ok(_) => Ok(()),
            Err(err) => {
                self.connecting = true;
                error!(target: "app", "publish_message - cannot publish, waiting for recovery...");
                let recovery_result = self.channel.as_ref().unwrap().wait_for_recovery(err).await;
                match recovery_result {
                    Ok(_) => {
                        self.connecting = false;
                        Err(AmqpError::ErrorButRecovered(String::from(
                            "amqp_client error, but connection recovered",
                        )))
                    }
                    Err(_) => {
                        self.connecting = false;
                        Err(AmqpError::ErrorCannotRecover(String::from(
                            "amqp_client error, cannot auto recover",
                        )))
                    }
                }
            }
        }
    }

    fn is_initialized(
        &self,
        check_connection: bool,
        check_channel: bool,
        check_queue: bool,
        check_consumer: bool,
    ) -> Result<(), AmqpError> {
        if check_connection && self.connection.is_none() {
            error!(target: "app", "is_initialized - amqp_client connection not initialized");
            return Err(AmqpError::Uninitialized(String::from(
                "amqp_client connection not initialized",
            )));
        }
        if check_channel && self.channel.is_none() {
            error!(target: "app", "is_initialized - amqp_client channel not initialized");
            return Err(AmqpError::Uninitialized(String::from(
                "amqp_client channel not initialized",
            )));
        }
        if check_queue && self.queue.is_none() {
            error!(target: "app", "is_initialized - amqp_client queue not initialized");
            return Err(AmqpError::Uninitialized(String::from(
                "amqp_client queue not initialized",
            )));
        }
        if check_consumer && self.consumer.is_none() {
            error!(target: "app", "is_initialized - amqp_client consumer not initialized");
            return Err(AmqpError::Uninitialized(String::from(
                "amqp_client consumer not initialized",
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::amqp::AmqpClient;
    use crate::config::{Env, init};
    use crate::errors::amqp_error::AmqpError;
    use pretty_assertions::assert_eq;

    #[test]
    #[test_log::test]
    fn wrong_is_initialized() {
        // init logger and env variables
        let env: Env = init();
        // create amqp_client without connecting it to the AMQP server
        let amqp_client =
            AmqpClient::new(env.amqp_uri.clone(), env.amqp_queue_name.clone()).consumer("consumer-tag".to_string());

        // cover all possible errors returned by the `is_initialized` method
        let mut res = amqp_client.is_initialized(true, true, true, true);
        assert_eq!(
            res.err().unwrap().to_string(),
            anyhow::Error::from(AmqpError::Uninitialized(String::from(
                "amqp_client connection not initialized",
            )))
            .to_string()
        );
        res = amqp_client.is_initialized(false, true, true, true);
        assert_eq!(
            res.err().unwrap().to_string(),
            anyhow::Error::from(AmqpError::Uninitialized(String::from(
                "amqp_client channel not initialized",
            )))
            .to_string()
        );
        res = amqp_client.is_initialized(false, false, true, true);
        assert_eq!(
            res.err().unwrap().to_string(),
            anyhow::Error::from(AmqpError::Uninitialized(String::from(
                "amqp_client queue not initialized",
            )))
            .to_string()
        );
        res = amqp_client.is_initialized(false, false, false, true);
        assert_eq!(
            res.err().unwrap().to_string(),
            anyhow::Error::from(AmqpError::Uninitialized(String::from(
                "amqp_client consumer not initialized",
            )))
            .to_string()
        );
    }
}
