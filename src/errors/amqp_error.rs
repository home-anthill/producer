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
