use thiserror::Error;

// custom error, based on 'thiserror' library
#[derive(Error, Debug)]
pub enum AmqpError {
    #[error("amqp_client not initialized error")]
    Uninitialized(String),
    #[error("amqp_client error, but connection recovered")]
    ErrorButRecovered(String),
    #[error("amqp_client error, cannot auto recover")]
    ErrorCannotRecover(String),
}
