use hmac::{Hmac, KeyInit, Mac};
use lapin::message::Delivery;
use lapin::options::{BasicAckOptions, BasicConsumeOptions};
use lapin::types::ShortString;
use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, Consumer, Queue,
    options::{BasicPublishOptions, QueueDeclareOptions},
    types::{AMQPValue, FieldTable},
};
use sha2::Sha256;
use tracing::{debug, error, info};
use uuid::Uuid;
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

fn compute_hmac(secret: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

use crate::errors::amqp_error::AmqpError;

/// Redacts credentials from an AMQP URI for safe logging.
/// Replaces everything between "://" and "@" with "[REDACTED]".
fn redact_uri(uri: &str) -> String {
    if let Some(at_pos) = uri.rfind('@')
        && let Some(scheme_end) = uri.find("://")
    {
        let scheme = &uri[..scheme_end + 3];
        let rest = &uri[at_pos..];
        return format!("{}[REDACTED]{}", scheme, rest);
    }
    "[REDACTED]".to_string()
}

pub struct AmqpClient {
    amqp_uri: Zeroizing<String>,
    hmac_secret: Zeroizing<String>,
    amqp_queue_name: ShortString,
    /// `None` means use the broker-assigned default consumer tag.
    consumer_tag: Option<ShortString>,
    properties: ConnectionProperties,
    connection: Option<Connection>,
    channel: Option<Channel>,
    queue: Option<Queue>,
    consumer: Option<Consumer>,
    connecting: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InitLevel {
    Connection,
    Channel,
    Queue,
    Consumer,
}

impl AmqpClient {
    pub fn new(amqp_uri: Zeroizing<String>, amqp_queue_name: String, hmac_secret: Zeroizing<String>) -> Self {
        Self {
            amqp_uri,
            hmac_secret,
            amqp_queue_name: amqp_queue_name.into(),
            properties: ConnectionProperties::default()
                .with_connection_name("amqp-client".into())
                .enable_auto_recover(),
            connection: None,
            channel: None,
            queue: None,
            connecting: false,
            consumer: None,
            consumer_tag: None,
        }
    }

    // Use the builder pattern to init an optional param
    #[must_use]
    pub fn consumer(mut self, consumer_tag: String) -> AmqpClient {
        self.consumer_tag = Some(consumer_tag.into());
        self
    }

    pub fn is_connected(&self, with_consumer: bool) -> bool {
        let level = if with_consumer {
            InitLevel::Consumer
        } else {
            InitLevel::Queue
        };
        if self.is_initialized(level).is_err() {
            return false;
        }
        self.connection_ref().is_ok_and(|c| c.status().connected())
            && self.channel_ref().is_ok_and(|c| c.status().connected())
    }

    pub async fn connect(&mut self, is_consumer: bool) -> Result<(), AmqpError> {
        info!(target: "app", "connect - trying to connect to amqp_uri={} with queue={}", redact_uri(&self.amqp_uri), &self.amqp_queue_name);
        self.connecting = true;
        self.create_connection().await?;
        info!(target: "app", "connect - creating channel...");
        self.create_channel().await?;
        info!(target: "app", "connect - declaring queue...");
        self.declare_queue().await?;
        if is_consumer {
            info!(target: "app", "connect - creating consumer...");
            self.create_consumer().await?;
        }
        self.connecting = false;
        info!(target: "app", "connect - AMQP connection done!");
        Ok(())
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
                return Err(AmqpError::ConnectionError("amqp_client connection error".into()));
            }
        };
        Ok(())
    }

    // private method that must be called after create_connection()
    async fn create_channel(&mut self) -> Result<(), AmqpError> {
        info!(target: "app", "create_channel - creating AMQP channel...");
        let conn = self.connection_ref()?;
        self.channel = match conn.create_channel().await {
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
        let ch = self.channel_ref()?.clone();
        self.queue = match ch
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

    // private method that must be called after create_connection(), create_channel(), and create_queue()
    async fn create_consumer(&mut self) -> Result<(), AmqpError> {
        info!(target: "app", "create_consumer - creating AMQP consumer...");
        let ch = self.channel_ref()?.clone();
        self.consumer = match ch
            .basic_consume(
                self.amqp_queue_name.clone(),
                self.consumer_tag.clone().unwrap_or_default(),
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
    pub async fn publish_message(&mut self, amqp_queue_name: &str, msg_byte: &[u8]) -> Result<(), AmqpError> {
        debug!(target: "app", "publish_message - publishing byte message to queue {}...", amqp_queue_name);
        if self.connecting {
            error!(target: "app", "publish_message - cannot publish while amqp_client is not initialized");
            return Err(AmqpError::Uninitialized(
                "cannot publish while amqp_client is not initialized".into(),
            ));
        }
        let channel = self.channel_ref()?.clone();

        let signature = compute_hmac(self.hmac_secret.as_bytes(), msg_byte);
        let mut headers = FieldTable::default();
        headers.insert(
            "x-hmac-sha256".into(),
            AMQPValue::LongString(hex::encode(&signature).into()),
        );

        let message_id = Uuid::new_v4().to_string();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let properties = BasicProperties::default()
            .with_headers(headers)
            .with_message_id(message_id.into())
            .with_timestamp(timestamp)
            // 30-second per-message TTL; RabbitMQ dead-letters expired messages,
            // limiting the replay window for captured messages.
            .with_expiration("30000".into());

        let publish_result = channel
            .basic_publish(
                "".into(),
                amqp_queue_name.into(),
                BasicPublishOptions::default(),
                msg_byte,
                properties,
            )
            .await;
        if let Err(err) = publish_result {
            error!(target: "app", "publish_message - cannot publish, waiting for recovery...");
            Err(self.wait_for_recovery(err).await)
        } else {
            Ok(())
        }
    }

    fn is_initialized(&self, level: InitLevel) -> Result<(), AmqpError> {
        if level >= InitLevel::Connection && self.connection.is_none() {
            error!(target: "app", "is_initialized - amqp_client connection not initialized");
            return Err(AmqpError::Uninitialized(
                "amqp_client connection not initialized".into(),
            ));
        }
        if level >= InitLevel::Channel && self.channel.is_none() {
            error!(target: "app", "is_initialized - amqp_client channel not initialized");
            return Err(AmqpError::Uninitialized("amqp_client channel not initialized".into()));
        }
        if level >= InitLevel::Queue && self.queue.is_none() {
            error!(target: "app", "is_initialized - amqp_client queue not initialized");
            return Err(AmqpError::Uninitialized("amqp_client queue not initialized".into()));
        }
        if level >= InitLevel::Consumer && self.consumer.is_none() {
            error!(target: "app", "is_initialized - amqp_client consumer not initialized");
            return Err(AmqpError::Uninitialized("amqp_client consumer not initialized".into()));
        }
        Ok(())
    }

    fn connection_ref(&self) -> Result<&Connection, AmqpError> {
        self.connection.as_ref().ok_or_else(|| {
            error!(target: "app", "connection_ref - amqp_client connection not initialized");
            AmqpError::Uninitialized("amqp_client connection not initialized".into())
        })
    }

    fn channel_ref(&self) -> Result<&Channel, AmqpError> {
        self.channel.as_ref().ok_or_else(|| {
            error!(target: "app", "channel_ref - amqp_client channel not initialized");
            AmqpError::Uninitialized("amqp_client channel not initialized".into())
        })
    }

    pub async fn close_connection(&mut self) -> Result<(), AmqpError> {
        let conn = self.connection_ref()?;
        conn.close(0, "".into())
            .await
            .map_err(|e| AmqpError::ConnectionError(format!("cannot close connection: {}", e)))
    }

    /// Waits for lapin to auto-recover the channel after a publish error.
    /// Always returns an `AmqpError` describing the outcome so the caller
    /// can propagate it with `Err(self.wait_for_recovery(err).await)`.
    pub async fn wait_for_recovery(&mut self, err: lapin::Error) -> AmqpError {
        info!(target: "app", "wait_for_recovery");
        let channel = match self.channel_ref() {
            Ok(ch) => ch.clone(),
            Err(e) => {
                error!(target: "app", "wait_for_recovery - not initialized: {}", e);
                return AmqpError::Uninitialized("amqp_client not initialized during recovery".into());
            }
        };
        self.connecting = true;
        let recovery_result = channel.wait_for_recovery(err).await;
        self.connecting = false;
        if recovery_result.is_ok() {
            AmqpError::ErrorButRecovered("amqp_client error, but connection recovered".into())
        } else {
            AmqpError::ErrorCannotRecover("amqp_client error, cannot auto recover".into())
        }
    }
}

pub async fn read_message(delivery: &Delivery) -> Result<&str, AmqpError> {
    delivery
        .ack(BasicAckOptions::default())
        .await
        .map_err(|e| AmqpError::ConnectionError(format!("cannot ack message: {}", e)))?;
    std::str::from_utf8(&delivery.data)
        .map_err(|e| AmqpError::ConnectionError(format!("cannot read payload as utf8: {}", e)))
}

#[cfg(test)]
mod tests {
    use crate::amqp::AmqpClient;
    use crate::config::{Env, init};
    use crate::errors::amqp_error::AmqpError;
    use pretty_assertions::assert_eq;
    use zeroize::Zeroizing;

    #[test]
    #[test_log::test]
    fn wrong_is_initialized() {
        let env: Env = init();
        let amqp_client = AmqpClient::new(
            env.amqp_uri.clone(),
            env.amqp_queue_name.clone(),
            Zeroizing::new(String::new()),
        )
        .consumer("consumer-tag".to_string());

        // When nothing is initialized, all levels fail at the connection check
        for level in [
            crate::amqp::InitLevel::Connection,
            crate::amqp::InitLevel::Channel,
            crate::amqp::InitLevel::Queue,
            crate::amqp::InitLevel::Consumer,
        ] {
            let res = amqp_client.is_initialized(level);
            assert_eq!(
                res.err().unwrap().to_string(),
                AmqpError::Uninitialized("amqp_client connection not initialized".into()).to_string()
            );
        }
    }
}
