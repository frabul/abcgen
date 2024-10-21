# abcgen - Actor's Boilerplate Code Generator
**abcgen** helps you to build Actor object by producing all the boilerplate code needed by this patter, meaning that all the code involved in defining/sending/raceiving/unwrapping messages and managing lifetime of the actor is hidden from user. <br> The user should only focus on the logic of the service that the actor is going to provide.
**abcgen** produces Actor objects that are based on the `async`/`await` syntax and the **tokio** library.
The actor objects generated do not require any scheduler o manager to run, they are standalone and can be used in any (**tokio**) context. 
```rust 
// Super quick example
#[abcgen::actor_module]
mod actor {
    use abcgen::{actor, message_handler, AbcgenError, PinnedFuture, Task};

    #[actor]
    pub struct MyActor {
        pub some_value: i32,
    }

    impl MyActor {
        pub async fn start(&mut self, task_sender: TaskSender) {
            println!("Starting");
            // here you can spawn some tasks using tokio::spawn
            // or enqueue some tasks into the actor's main loop by sending them to task_sender
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                task_sender
                    .send(Box::new(MyActor::example_task))
                    .await
                    .unwrap();
            });
        }
        
        pub async fn shutdown(&mut self) {
            println!("Shutting down");
        }

        #[message_handler]
        async fn get_value(&mut self, name: &'static str) -> Result<i32, &'static str> {
            match name {
                "some_value" => Ok(self.some_value),
                _ => Err("Invalid name"),
            }
        }

        fn example_task(&mut self) -> PinnedFuture<()> {
            Box::pin(async move {
                println!("Example task executed");
            })
        }
    }
}

#[tokio::main]
async fn main() {
    let actor: actor::MyActorProxy = actor::MyActor { some_value: 32 }.run();
    let the_value = actor.get_value("some_value").await.unwrap();
    assert!(matches!(the_value, Ok(32)));
    let the_value = actor.get_value("some_other_value").await.unwrap();
    assert!(matches!(the_value, Err("Invalid name")));
}

```
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
- `Actor::invoke(...)` helper method to enqueue a task to be executed in the `actor`'s loop 
- an ActorProxy object that implements all of the methods that were marked with `message_handler` attribute
- a message enum that contains all the messages that the `actor` can receive  (which is not meant to be used directly by the user)

More details can be found in the example below.

## Example
You can have a look at [generated code] for this example to see what **abcgen** produces.<br>
More examples can be found in the [examples directory] of the repository.

[generated code]: https://github.com/frabul/abcgen/blob/main/examples/hello_world_expanded.rs

[examples directory]: https://github.com/frabul/abcgen/blob/main/examples/
 

```rust
// the following attribute is actually emitting the code by calling a procedural macro
#[abcgen::actor_module] 
mod hello_wordl_actor {
    use abcgen::*;

    // the follow attribute is used to mark the enum defining 
    // the events that can be raised by the actor
    // events are optional, they can be omitted
    #[events] 
    #[derive(Debug, Clone)]
    pub enum HelloWorldActorEvent {
        SomeoneAskedMyName(String),
        Message(String),
    }

    #[actor] // this attribute is used to mark the struct defining the actor
    pub struct HelloWorldActor {
        pub event_sender: Option<EventSender>,
    }

    impl HelloWorldActor {
        // following function *must* be implemented by the
        // user and is called by the run function
        async fn start(
            &mut self,
            // this object can be used to send a function to be invoked in the actor's loop
            // see Self::invoke
            task_sender: TaskSender,  
            // this argument should be removed if there are no events
            event_sender: EventSender, 
        ) {
            self.event_sender = Some(event_sender);
            println!("Hello, World!");

            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                // the following call will send the function 
                // Self::still_here to be invoked by the actor's loop
                //
                // ⚠️ Do not put any sleep in Self::still_here, 
                // it will block the actor's loop
                Self::invoke(&task_sender, Box::new(Self::still_here)).unwrap(); 
            });
        }

        // following function must be implemented by 
        // the user and is called befor termination
        async fn shutdown(&mut self) {
            println!("Goodbye, World!");
        }

        // following function is a meant to handle a message
        // abcgen generates a message for it
        // something like 
        // HelloWorldActorMessage::TellMeYourName({
        //      caller: String, 
        //      respond_to: tokio::sync::oneshot::Sender<String>})
        //
        // ⚠️ Do not put any sleep in this function, it will block the actor's task
        #[message_handler]
        async fn tell_me_your_name(&mut self, caller: String) -> String {
            self.event_sender
                .as_ref()
                .unwrap()
                .send(HelloWorldActorEvent::SomeoneAskedMyName(caller.clone()))
                .unwrap();
            println!("Hello {}, I am HelloWorldActor", caller);
            "HelloWorldActor".to_string()
        }

        // following function can be enqueued as a task
        // to executed in the actor's task
        // ⚠️ Do not put any sleep in this function, 
        //    it will block the actor's task
        fn still_here(&mut self) -> PinnedFuture<()> {
            Box::pin(async {
                self.event_sender
                    .as_ref()
                    .unwrap()
                    .send(HelloWorldActorEvent::Message(
                        "Hello world again, I'm still here.".to_string(),
                    ))
                    .unwrap();
                // don't do this 
                // tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            })
        }
    }
}

// ---- main.rs ----
use hello_wordl_actor::{HelloWorldActor, HelloWorldActorEvent};

#[tokio::main]
async fn main() {
    let actor = HelloWorldActor { event_sender: None };
    // the following call will spawn a tokio task that will handle
    // the messages received by the actor
    // it consumes the actor and returns a proxy that can be used
    // to send and receive messages
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
    // this call, under the hood, will send a message to the actor and wait for the reply 
    let thename = proxy.tell_me_your_name("Alice".to_string()).await.unwrap();
    println!("The actor replied with name: \"{}\"", thename);
    
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
}

```


