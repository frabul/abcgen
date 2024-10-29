# abcgen - Actor's Boilerplate Code Generator
**abcgen** helps you to build Actor object by producing all the boilerplate code needed by this patter, meaning that all the code involved in defining/sending/raceiving/unwrapping messages and managing lifetime of the actor is hidden from user. <br> The user should only focus on the logic of the service that the actor is going to provide.
**abcgen** produces Actor objects that are based on the `async`/`await` syntax and the **tokio** library.
The actor objects generated do not require any scheduler o manager to run, they are standalone and can be used in any (**tokio**) context. 

## The user should provide:
- a struct or enum definition marked with `actor` attribute 
- implement start(...) and shutdown(...) methods for the `actor`
- implement, for the `actor`, a set of methods marked with `message_handler` attribute; these are going to handle the messages that the `actor` can receive.
- optionally, an enum definition marked with `events` attribute to define the events that the `actor` can signal

## The procedural macro will generate:
- implementation of `run(self)` method for the `actor` which will return an ActorProxy
- implementation of message handling logic for the `actor`: 
    - calling the `start(...)` method before entering the `actor`'s loop
    - calling the `shutdown(&mut self)` method after exiting the `actor`'s loop
    - handling of stop signal
    - handling of messages (support replies)
    - handling of tasks (functions that can be enqueued to be invoked in the `actor`'s loop so the can access `&mut Actor`)
- an ActorProxy object that implements all of the methods that were marked with `message_handler` attribute
- a message enum that contains all the messages that the `actor` can receive  (which is not meant to be used directly by the user)

More details can be found in the example below.

## Example
You can have a look at [generated code] for this example to see what **abcgen** produces.<br>
More examples can be found in the [examples directory] of the repository.

[generated code]: https://github.com/frabul/abcgen/blob/main/examples/hello_world_expanded.rs

[examples directory]: https://github.com/frabul/abcgen/blob/main/examples/
 

```rust
// below there is the attribute that is actually emitting the code by calling a procedural macro
#[abcgen::actor_module(channels_size = 123, events_chan_size = 15)] 
#[allow(unused)]
mod hello_world_actor {
    use abcgen::*;

    // Events are optional, if they are not found the start method will not receive the event_sender argument
    #[events] // this attribute is used to mark the enum defining the events that can be signaled by the actor
    #[derive(Debug, Clone)]
    pub enum HelloWorldActorEvent {
        SomeoneAskedMyName(String),
        Message(String),
    }

    /// Some errors that we want to return in our actor's handler methods
    #[derive(thiserror::Error, Debug)]
    pub enum HelloWorldError {
        #[error("Actor already stopped")]
        AlreadyStopped,
        #[error("HelloWorldErrors 1")]
        Error1,
        #[error("HelloWorldErrors 2")]
        Error2,
    }

    /// It is useful to implement the From<AbcgenError> for the errors
    /// that can be returned by the actor's handler methods so that
    /// the proxy can return a flat Result<V, E> instead
    /// of a nested Result<Result<V, E>, AbcgenError>
    impl From<AbcgenError> for HelloWorldError {
        fn from(_: abcgen::AbcgenError) -> Self {
            HelloWorldError::AlreadyStopped
        }
    }

    #[derive(thiserror::Error, Debug)]
    pub enum SomeOtherError {
        #[error("SomeOtherErrors 1")]
        Error1,
        #[error("SomeOtherErrors 2")]
        Error2,
    }

    #[actor] // this attribute is used to mark the struct defining the actor
    pub struct HelloWorldActor {
        pub event_sender: Option<EventSender>,
    }

    impl HelloWorldActor {
        /// The following function *must* be implemented by the user and is called by the run function
        async fn start(
            &mut self,
            task_sender: TaskSender, // this can be used to send a function to be invoked in the actor's loop
            event_sender: EventSender, // this argument should be removed if there are no events
        ) {
            self.event_sender = Some(event_sender);
            println!("Hello, World!");

            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                // the following call will send the function Self::still_here to be invoked by the actor's loop
                send_task!(task_sender(this) => {
                     // ⚠️ Do not put any sleep in the closure or Self::still_here, it will block the actor's loop
                     this.still_here().await;
                });
            });
        }

        /// The following function must be implemented by the user and is called befor termination
        async fn shutdown(&mut self) {
            println!("Goodbye, World!");
        }

        /// following function is meant to handle a message because it is marked with `#[message_handler]`
        /// abcgen generates a message for it
        /// that should have the following signature:
        /// ```
        /// HelloWorldActorMessage::TellMeYourName({caller: String, respond_to: tokio::sync::oneshot::Sender<Result<String, HelloWorldError>>})
        /// ```
        /// A specular function is generated on the proxy that can be called to send the message and receive the response.
        /// In this case the fuction of the proxy will return the same `Result<String, HelloWorldError>` because there is
        /// a conversion `From<AbcgenError>` for HelloWorldError otherwise it would return a nested `Result<Result<String, HelloWorldError>, AbcgenError>`
        ///
        #[message_handler]
        async fn tell_me_your_name(&mut self, caller: String) -> Result<String, HelloWorldError> {
            // ⚠️ Do not put any sleep in this function, it will block the actor's task
            self.event_sender
                .as_ref()
                .unwrap()
                .send(HelloWorldActorEvent::SomeoneAskedMyName(caller.clone()))
                .unwrap();
            println!("Hello {}, I am HelloWorldActor", caller);
            Ok("HelloWorldActor".to_string())
        }
        /// The following function is meant to handle a message because it is marked with `#[message_handler]`
        /// In this case the fuction generated on the proxy will return a nested `Result<Result<(), SomeOtherError>, AbcgenError>`
        #[message_handler]
        async fn do_that(&mut self) -> Result<(), SomeOtherError> {
            println!("do_that called");
            Ok(())
        }

        /// The following function can be enqueued as a task to executed in the actor's task
        fn still_here(&mut self) -> PinnedFuture<()> {
            // ⚠️ Do not put any sleep in this function, it will block the actor's task
            Box::pin(async {
                self.event_sender
                    .as_ref()
                    .unwrap()
                    .send(HelloWorldActorEvent::Message(
                        "Hello world again, I'm still here.".to_string(),
                    ))
                    .unwrap();
                // don't do this tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            })
        }
    }
}

use abcgen::AbcgenError;
// ---- main.rs ----
use hello_world_actor::{HelloWorldActor, HelloWorldActorEvent, SomeOtherError};

#[tokio::main]
async fn main() {
    let actor = HelloWorldActor { event_sender: None };
    // the following call will spawn a tokio task that will handle the messages received by the actor
    // it consumes the actor and returns a proxy that can be used to send and receive messages
    let proxy = actor.run();

    // handle events sent by the actor
    let mut events_rx = proxy.get_events().resubscribe();
    tokio::spawn(async move {
        while let Ok(event) = events_rx.recv().await {
            match event {
                HelloWorldActorEvent::SomeoneAskedMyName(name) => {
                    println!("{} asked my name", name);
                }
                HelloWorldActorEvent::Message(msg) => {
                    println!("Actor said: \"{}\"", msg);
                }
            }
        }
    });

    // in the case of SomeOtherError there is no conversion to AbcgenError so the method returns
    // a nested Result
    let do_that_res: Result<Result<(), SomeOtherError>, AbcgenError> = proxy.do_that().await;

    // in the case of HelloWorldError there is a conversion to AbcgenError so the method returns
    // a flat result because the eventual AbcgenError is converted to HelloWorldError
    // thanks to the From<AbcgenError> implementation
    let thename = proxy.tell_me_your_name("Alice".to_string()).await.unwrap();
    println!("The actor replied with name: \"{}\"", thename);

    match do_that_res {
        Ok(Ok(_)) => println!("do_that succeeded"),
        Ok(Err(e)) => println!("do_that failed: {:?}", e),
        Err(e) => println!("do_that failed: {:?}", e),
    }
    // -- wait for events
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
}


```


