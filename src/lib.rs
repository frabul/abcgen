//! # abcgen - Actor's Boilerplate Code Generator
//! **abcgen** helps you to build Actor object by producing all the boilerplate code needed by this patter, meaning that all the code involved in defining/sending/raceiving/unwrapping messages and managing lifetime of the actor is hidden from user. <br>
//! The user should only focus on the logic of the service that the actor is going to provide.
//! **abcgen** produces Actor objects that are based on the `async`/`await` syntax and the **tokio** library.
//! The actor objects generated do not require any scheduler o manager to run, they are standalone and can be used in any (**tokio**) context.
//! 
//! ## Basic example
//! The following example is minimale and does not shocase all the features of the library.
//! Check the [README] for more details.
//! ```rust
//! #[abcgen::actor_module]
//! #[allow(unused)]
//! mod actor {
//!     use abcgen::{actor, message_handler, send_task, AbcgenError, PinnedFuture, Task};
//!
//!     #[actor]
//!     pub struct MyActor {
//!         pub some_value: i32,
//!     }
//!
//!     impl MyActor {
//!         pub async fn start(&mut self, task_sender: TaskSender) {
//!             println!("Starting");
//!             // here you can spawn some tasks using tokio::spawn
//!             // or enqueue some tasks into the actor's main loop by sending them to task_sender
//!             tokio::spawn(async move {
//!                 tokio::time::sleep(std::time::Duration::from_secs(1)).await;
//!                 send_task!( task_sender(this) => { this.example_task().await; } );
//!             });
//!         }
//!         pub async fn shutdown(&mut self) {
//!             println!("Shutting down");
//!         }
//!         #[message_handler]
//!         async fn get_value(&mut self, name: &'static str) -> Result<i32, &'static str> {
//!             match name {
//!                 "some_value" => Ok(self.some_value),
//!                 _ => Err("Invalid name"),
//!             }
//!         }
//!
//!         async fn example_task(&mut self) {
//!             println!("Example task executed");
//!         }
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let actor: actor::MyActorProxy = actor::MyActor { some_value: 32 }.run();
//!     let the_value = actor.get_value("some_value").await.unwrap();
//!     assert!(matches!(the_value, Ok(32)));
//!     let the_value = actor.get_value("some_other_value").await.unwrap();
//!     assert!(matches!(the_value, Err("Invalid name")));
//! }
//!
//! ```
//! ## The user should provide:
//! - a struct or enum definition marked with `actor` attribute
//! - implement start(...) and shutdown(...) methods for the `actor`
//! - implement, for the `actor`, a set of methods marked with `message_handler` attribute; these are going to handle the messages that the `actor` can receive.
//! - optionally, an enum definition marked with `events` attribute to define the events that the `actor` can signal
//!
//! ## The procedural macro will generate:
//! - implementation of `run(self)` method for the `actor` which will return an ActorProxy
//! - implementation of message handling logic for the `actor`:
//!     - calling the `start(...)` method before entering the `actor`'s loop
//!     - calling the `shutdown(&mut self)` method after exiting the `actor`'s loop
//!     - handling of stop signal
//!     - handling of messages (support replies)
//!     - handling of tasks (functions that can be enqueued to be invoked in the `actor`'s loop so the can access `&mut Actor`)
//! - `Actor::invoke(...)` helper method to enqueue a task to be executed in the `actor`'s loop
//! - an ActorProxy object that implements all of the methods that were marked with `message_handler` attribute
//! - a message enum that contains all the messages that the `actor` can receive  (which is not meant to be used directly by the user)
//!
//! More details can be found in the [README] file.
//!
//! [README]: https://github.com/frabul/abcgen/blob/main/README.md
use std::{future::Future, pin::Pin};

pub use abcgen_attributes::*;

/// Helper type for async tasks
/// The tasks that are meant to be sent to the actor need to return this type
pub type PinnedFuture<'a, TResult> = Pin<Box<dyn Future<Output = TResult> + Send + 'a>>;

/// The type of the tasks that are sent to the actor
pub type Task<TActor> = Box<dyn (for<'b> FnOnce(&'b mut TActor) -> PinnedFuture<'b, ()>) + Send>;

/// A macro to simplify sending tasks to the actor
/// Following below example you need to provide a refence to `task_sender` and  
/// the name for the reference to the actor (`this` in the example).
/// The macro will create a closure that returx a `PinnedFuture` expressed with |this| Box::pin(async move { ... })
/// and send it trough the `task_sender`.
/// Example:
/// ```rust
/// send_task!(task_sender => (this) {
///    this.some_field = 123;
/// });
/// ```
#[macro_export]
macro_rules! send_task {
    ($task_sender:ident ($this:ident ) => { $($tok:tt)* }) => {
        let _ = $task_sender.send(Box::new(|$this| Box::pin( async move { $($tok)* }))).await;
    };
}

/// Error type for the code generated by the abcgen
#[derive(Debug, thiserror::Error)]
pub enum AbcgenError {
    #[error("Service is already stopped")]
    ActorShutdown,
    #[error("Failed to send message. Channel overrun or service stopped.")]
    ChannelError(#[source] Box<dyn std::error::Error>),
}
