// use log::info;
// use std::string::String;
//
// use paho_mqtt::{AsyncClient, AsyncReceiver, ConnectOptions, Message, ServerResponse};
// use producer::mqtt::mqtt_client::MqttClient;
// use producer::mqtt::mqtt_options::MqttOptions;
//
// pub struct MqttClientStub {
//     conn_opts: ConnectOptions,
//     client: AsyncClient,
//     pub message_stream: AsyncReceiver<Option<Message>>,
// }
//
// impl MqttClient for MqttClientStub {
//     fn new(options: MqttOptions) -> Result<Self, anyhow::Error> {
//         let mut client: AsyncClient = AsyncClient::new(options.create_opts)?;
//         // Get message stream before connecting
//         let message_stream = client.get_stream(25);
//         Ok(Self {
//             conn_opts: options.conn_opts,
//             client,
//             message_stream,
//         })
//     }
//
//     async fn connect(&mut self) {
//         ()
//     }
//
//     async fn reconnect(&self) -> paho_mqtt::Result<ServerResponse> {
//         todo!()
//     }
//
//     // async fn reconnect(&self) -> paho_mqtt::Result<ServerResponse> {
//     //     Ok(ServerResponse{
//     //         rsp: Default::default(),
//     //         props: Default::default(),
//     //         reason_code: Default::default(),
//     //     })
//     // }
//
//     async fn subscribe(&mut self, topics_list: &[&str]) -> Result<(), paho_mqtt::Error> {
//         info!(target: "app", "subscribe - Subscribing to the topics...");
//         let topics: Vec<String> = topics_list.iter().map(|s| s.to_string()).collect();
//         info!(target: "app", "subscribe - Subscribing to MQTT topics: {:?}", topics);
//         Ok(())
//     }
//
//     // pub async fn trigger_msg(&self) {
//     //     self.message_stream.
//     // }
// }
