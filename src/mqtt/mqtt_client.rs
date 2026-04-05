use futures::stream::StreamExt;
use paho_mqtt::{AsyncClient, AsyncReceiver, ConnectOptions, Message, ServerResponse};
use tracing::{error, info};

use crate::mqtt::mqtt_options::MqttOptions;

pub struct MqttClient {
    conn_opts: ConnectOptions,
    client: AsyncClient,
    message_stream: AsyncReceiver<Option<Message>>,
}

impl MqttClient {
    pub fn new(options: MqttOptions) -> Result<Self, anyhow::Error> {
        let mut client: AsyncClient = AsyncClient::new(options.create_opts)?;
        let message_stream = client.get_stream(25);
        Ok(Self {
            conn_opts: options.conn_opts,
            client,
            message_stream,
        })
    }

    pub async fn connect(&mut self) -> Result<(), paho_mqtt::Error> {
        info!(target: "app", "connect - Connecting to the MQTT server with ConnectOptions...");
        self.client.connect(self.conn_opts.clone()).await?;
        info!(target: "app", "connect - MQTT Connection succeeded");
        Ok(())
    }

    pub async fn reconnect(&self) -> paho_mqtt::Result<ServerResponse> {
        info!(target: "app", "reconnect - Reconnecting to the MQTT server...");
        self.client.reconnect().await
    }

    pub async fn subscribe<S: AsRef<str>>(&mut self, topics_list: &[S]) -> Result<(), paho_mqtt::Error> {
        let topics: Vec<String> = topics_list.iter().map(|s| s.as_ref().to_owned()).collect();
        info!(target: "app", "subscribe - Subscribing to the topics: {:?}", topics);
        let qos = vec![1; topics.len()];
        match self.client.subscribe_many(&topics, &qos).await {
            Ok(_) => {
                info!(target: "app", "subscribe - Subscription to the topics completed");
                Ok(())
            }
            Err(err) => {
                error!(target: "app", "subscribe - Cannot subscribe to topics. Error = {:?}", err);
                Err(err)
            }
        }
    }

    pub async fn get_next_message(&mut self) -> Option<Option<Message>> {
        self.message_stream.next().await
    }

    pub async fn disconnect(&mut self) -> paho_mqtt::Result<ServerResponse> {
        self.client.disconnect(None).await
    }
}
