use thiserror::Error;

#[derive(Error, Debug)]
pub enum AmqpError {
    #[error("amqp_client not initialized: {0}")]
    Uninitialized(String),
    #[error("amqp_client connection error: {0}")]
    ConnectionError(String),
    #[error("amqp_client error, but connection recovered: {0}")]
    ErrorButRecovered(String),
    #[error("amqp_client error, cannot auto recover: {0}")]
    ErrorCannotRecover(String),
}
