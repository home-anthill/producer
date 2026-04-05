use thiserror::Error;

#[derive(Error, Debug)]
pub enum MessageError {
    #[error("Received empty message error")]
    EmptyMessageError,
}
