use std::{future::Future, pin::Pin};

pub use abcgen_attributes::*;

pub type PinnedFuture<'a, TResult> = Pin<Box<dyn Future<Output = TResult> + Send + 'a>>;

pub type Task<TActor> = Box<dyn (for<'b> Fn(&'b mut TActor) -> PinnedFuture<'b, ()>) + Send>;

#[derive(Debug, thiserror::Error)]
pub enum AbcgenError {
    #[error("Service is already stopped")]
    AlreadyStopped,
    #[error("Failed to send message. Channel overrun or service stopped.")]
    ChannelError(#[source] Box<dyn std::error::Error>),
}
