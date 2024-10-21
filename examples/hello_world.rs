#[abcgen::actor_module] // this is the attribute that is actually emitting the code by calling a procedural macro
#[allow(unused)]
mod hello_wordl_actor {
    use abcgen::*;

    // the follow attribute is used to mark the enum defining the events that can be signaled by the actor
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
        // following function *must* be implemented by the user and is called by the run function
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
                // ⚠️ Do not put any sleep in Self::still_here, it will block the actor's loop
                Self::invoke(&task_sender, Box::new(Self::still_here)).unwrap();
            });
        }

        // following function must be implemented by the user and is called befor termination
        async fn shutdown(&mut self) {
            println!("Goodbye, World!");
        }

        // following function is a meant to handle a message
        // abcgen generates a message for it
        // something like HelloWorldActorMessage::TellMeYourName({caller: String, respond_to: tokio::sync::oneshot::Sender<String>})
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

        // following function can be enqueued as a task to executed in the actor's task
        // ⚠️ Do not put any sleep in this function, it will block the actor's task
        fn still_here(&mut self) -> PinnedFuture<()> {
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

// ---- main.rs ----
use hello_wordl_actor::{HelloWorldActor, HelloWorldActorEvent};

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

    let thename = proxy.tell_me_your_name("Alice".to_string()).await.unwrap();
    println!("The actor replied with name: \"{}\"", thename);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
}
