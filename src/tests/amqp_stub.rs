// use std::string::String;
//
// pub struct AmqpClient {
//     pub amqp_uri: String,
//     pub amqp_queue_name: String,
// }
//
// impl AmqpClient {
//     pub fn new(amqp_uri: String, amqp_queue_name: String) -> Self {
//         Self {
//             amqp_uri,
//             amqp_queue_name,
//         }
//     }
//
//     pub async fn connect_with_retry_loop(&mut self) {}
//
//     pub async fn publish_message(&self, msg_byte: Vec<u8>) -> Result<(), anyhow::Error> {
//         Ok(())
//     }
//
//     pub fn is_connected(&self) -> bool {
//         true
//     }
// }
