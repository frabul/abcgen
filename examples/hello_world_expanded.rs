#[allow(unused)]
mod hello_world_actor {
    use abcgen::*;
    #[events]
    #[derive(Debug, Clone)]
    pub enum HelloWorldActorEvent {
        SomeoneAskedMyName(String),
        Message(String),
    }
    #[doc = " Some errors that we want to return in our actor's handler methods"]
    #[derive(thiserror :: Error, Debug)]
    pub enum HelloWorldError {
        #[error("Actor already stopped")]
        AlreadyStopped,
        #[error("HelloWorldErrors 1")]
        Error1,
        #[error("HelloWorldErrors 2")]
        Error2,
    }
    #[doc = " It is useful to implement the From<AbcgenError> for the errors"]
    #[doc = " that can be returned by the actor's handler methods so that"]
    #[doc = " the proxy can return a flat Result<V, E> instead"]
    #[doc = " of a nested Result<Result<V, E>, AbcgenError>"]
    impl From<AbcgenError> for HelloWorldError {
        fn from(_: abcgen::AbcgenError) -> Self {
            HelloWorldError::AlreadyStopped
        }
    }
    #[derive(thiserror :: Error, Debug)]
    pub enum SomeOtherError {
        #[error("SomeOtherErrors 1")]
        Error1,
        #[error("SomeOtherErrors 2")]
        Error2,
    }
    #[actor]
    pub struct HelloWorldActor {
        pub event_sender: Option<EventSender>,
    }
    impl HelloWorldActor {
        #[doc = " The following function *must* be implemented by the user and is called by the run function"]
        async fn start(&mut self, task_sender: TaskSender, event_sender: EventSender) {
            self.event_sender = Some(event_sender);
            println!("Hello, World!");
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                send_task ! (task_sender (this) => { this . still_here () . await ; });
            });
        }
        #[doc = " The following function must be implemented by the user and is called befor termination"]
        async fn shutdown(&mut self) {
            println!("Goodbye, World!");
        }
        #[doc = " following function is meant to handle a message because it is marked with `#[message_handler]`"]
        #[doc = " abcgen generates a message for it"]
        #[doc = " that should have the following signature:"]
        #[doc = " ```"]
        #[doc = " HelloWorldActorMessage::TellMeYourName({caller: String, respond_to: tokio::sync::oneshot::Sender<Result<String, HelloWorldError>>})"]
        #[doc = " ```"]
        #[doc = " A specular function is generated on the proxy that can be called to send the message and receive the response."]
        #[doc = " In this case the fuction of the proxy will return the same `Result<String, HelloWorldError>` because there is"]
        #[doc = " a conversion `From<AbcgenError>` for HelloWorldError otherwise it would return a nested `Result<Result<String, HelloWorldError>, AbcgenError>`"]
        #[doc = ""]
        #[message_handler]
        async fn tell_me_your_name(&mut self, caller: String) -> Result<String, HelloWorldError> {
            self.event_sender
                .as_ref()
                .unwrap()
                .send(HelloWorldActorEvent::SomeoneAskedMyName(caller.clone()))
                .unwrap();
            println!("Hello {}, I am HelloWorldActor", caller);
            Ok("HelloWorldActor".to_string())
        }
        #[doc = " The following function is meant to handle a message because it is marked with `#[message_handler]`"]
        #[doc = " In this case the fuction generated on the proxy will return a nested `Result<Result<(), SomeOtherError>, AbcgenError>`"]
        #[message_handler]
        async fn do_that(&mut self) -> Result<(), SomeOtherError> {
            println!("do_that called");
            Ok(())
        }
        #[doc = " The following function can be enqueued as a task to executed in the actor's task"]
        fn still_here(&mut self) -> PinnedFuture<()> {
            Box::pin(async {
                self.event_sender
                    .as_ref()
                    .unwrap()
                    .send(HelloWorldActorEvent::Message(
                        "Hello world again, I'm still here.".to_string(),
                    ))
                    .unwrap();
            })
        }
    }
    type EventSender = tokio::sync::broadcast::Sender<HelloWorldActorEvent>;
    pub type TaskSender = tokio::sync::mpsc::Sender<Task<HelloWorldActor>>;
    #[derive(Debug)]
    pub enum HelloWorldActorMessage {
        TellMeYourName {
            caller: String,
            respond_to: tokio::sync::oneshot::Sender<Result<String, HelloWorldError>>,
        },
        DoThat {
            respond_to: tokio::sync::oneshot::Sender<Result<(), SomeOtherError>>,
        },
    }
    impl HelloWorldActor {
        pub fn run(self) -> HelloWorldActorProxy {
            let (msg_sender, mut msg_receiver) = tokio::sync::mpsc::channel(10usize);
            let (event_sender, _) =
                tokio::sync::broadcast::channel::<HelloWorldActorEvent>(10usize);
            let event_sender_clone = event_sender.clone();
            let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel::<()>();
            let (task_sender, mut task_receiver) =
                tokio::sync::mpsc::channel::<Task<HelloWorldActor>>(10usize);
            tokio::spawn(async move {
                let mut actor = self;
                actor.start(task_sender, event_sender_clone).await;
                tokio::select! { _ = actor . select_receivers (& mut msg_receiver , & mut task_receiver) => { log :: debug ! ("(abcgen) all proxies dropped") ; } _ = stop_receiver => { log :: debug ! ("(abcgen) stop signal received") ; } }
                actor.shutdown().await;
            });
            let proxy = HelloWorldActorProxy {
                message_sender: msg_sender,
                stop_signal: Some(stop_sender),
                events: event_sender,
            };
            proxy
        }
        async fn select_receivers(
            &mut self,
            msg_receiver: &mut tokio::sync::mpsc::Receiver<HelloWorldActorMessage>,
            task_receiver: &mut tokio::sync::mpsc::Receiver<Task<HelloWorldActor>>,
        ) {
            loop {
                tokio::select! { msg = msg_receiver . recv () => { match msg { Some (msg) => { self . dispatch (msg) . await ; } None => { break ; } } } , task = task_receiver . recv () => { if let Some (task) = task { task (self) . await ; } } }
            }
        }
        async fn dispatch(&mut self, message: HelloWorldActorMessage) {
            match message {
                HelloWorldActorMessage::TellMeYourName { caller, respond_to } => {
                    let result = self.tell_me_your_name(caller).await;
                    respond_to.send(result).unwrap();
                }
                HelloWorldActorMessage::DoThat { respond_to } => {
                    let result = self.do_that().await;
                    respond_to.send(result).unwrap();
                }
            }
        }
    }
    pub struct HelloWorldActorProxy {
        message_sender: tokio::sync::mpsc::Sender<HelloWorldActorMessage>,
        events: tokio::sync::broadcast::Sender<HelloWorldActorEvent>,
        stop_signal: std::option::Option<tokio::sync::oneshot::Sender<()>>,
    }
    impl HelloWorldActorProxy {
        pub fn is_running(&self) -> bool {
            match self.stop_signal.as_ref() {
                Some(s) => !s.is_closed(),
                None => false,
            }
        }
        pub fn stop(&mut self) -> Result<(), AbcgenError> {
            match self.stop_signal.take() {
                Some(tx) => tx.send(()).map_err(|_e: ()| AbcgenError::ActorShutDown),
                None => Err(AbcgenError::ActorShutDown),
            }
        }
        pub async fn stop_and_wait(&mut self) -> Result<(), AbcgenError> {
            self.stop()?;
            while self.is_running() {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Ok(())
        }
        pub fn get_events(&self) -> tokio::sync::broadcast::Receiver<HelloWorldActorEvent> {
            self.events.subscribe()
        }
        pub async fn tell_me_your_name(&self, caller: String) -> Result<String, HelloWorldError> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let msg = HelloWorldActorMessage::TellMeYourName {
                caller,
                respond_to: tx,
            };
            let send_res = self.message_sender.send(msg).await;
            match send_res {
                Ok(_) => rx
                    .await
                    .unwrap_or_else(|e| Err(AbcgenError::ActorShutDown.into())),
                Err(e) => Err(AbcgenError::ActorShutDown.into()),
            }
        }
        pub async fn do_that(&self) -> Result<Result<(), SomeOtherError>, AbcgenError> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let msg = HelloWorldActorMessage::DoThat { respond_to: tx };
            let send_res = self.message_sender.send(msg).await;
            match send_res {
                Ok(_) => rx.await.map_err(|e| AbcgenError::ActorShutDown),
                Err(e) => Err(AbcgenError::ActorShutDown),
            }
        }
    }
}
use abcgen::AbcgenError;
use hello_world_actor::{HelloWorldActor, HelloWorldActorEvent, SomeOtherError};
#[tokio::main]
async fn main() {
    let actor = HelloWorldActor { event_sender: None };
    let proxy = actor.run();
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
    let do_that_res: Result<Result<(), SomeOtherError>, AbcgenError> = proxy.do_that().await;
    let thename = proxy.tell_me_your_name("Alice".to_string()).await.unwrap();
    println!("The actor replied with name: \"{}\"", thename);
    match do_that_res {
        Ok(Ok(_)) => println!("do_that succeeded"),
        Ok(Err(e)) => println!("do_that failed: {:?}", e),
        Err(e) => println!("do_that failed: {:?}", e),
    }
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
}
