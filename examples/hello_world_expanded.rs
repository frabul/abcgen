//#[abcgen::actor_module]
#[allow(dead_code)]
mod hello_wordl_actor {

    use abcgen::*;
    #[events]
    #[derive(Debug, Clone)]
    pub enum HelloWorldActorEvent {
        SomeoneAskedMyName(String),
        Message(String),
    }
    #[actor]
    pub struct HelloWorldActor {
        pub event_sender: Option<tokio::sync::broadcast::Sender<HelloWorldActorEvent>>,
    }
    impl HelloWorldActor {
        async fn start(
            &mut self,
            task_sender: tokio::sync::mpsc::Sender<Task<HelloWorldActor>>,
            event_sender: tokio::sync::broadcast::Sender<HelloWorldActorEvent>,
        ) {
            self.event_sender = Some(event_sender);
            println!("Hello, World!");
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                Self::invoke(&task_sender, Box::new(Self::still_here)).unwrap();
            });
        }
        async fn shutdown(&mut self) {
            println!("Goodbye, World!");
        }
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
    #[derive(Debug)]
    pub enum HelloWorldActorMessage {
        TellMeYourName {
            caller: String,
            respond_to: tokio::sync::oneshot::Sender<String>,
        },
    }
    pub struct HelloWorldActorProxy {
        message_sender: tokio::sync::mpsc::Sender<HelloWorldActorMessage>,
        events: tokio::sync::broadcast::Receiver<HelloWorldActorEvent>,
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
                Some(tx) => tx.send(()).map_err(|_e: ()| AbcgenError::AlreadyStopped),
                None => Err(AbcgenError::AlreadyStopped),
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
            self.events.resubscribe()
        }
        pub async fn tell_me_your_name(&self, caller: String) -> Result<String, AbcgenError> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let msg = HelloWorldActorMessage::TellMeYourName {
                caller,
                respond_to: tx,
            };
            let send_res = self.message_sender.send(msg).await;
            match send_res {
                Ok(_) => rx.await.map_err(|e| AbcgenError::ChannelError(Box::new(e))),
                Err(e) => Err(AbcgenError::ChannelError(Box::new(e))),
            }
        }
    }
    impl HelloWorldActor {
        pub fn run(self) -> HelloWorldActorProxy {
            let (msg_sender, mut msg_receiver) = tokio::sync::mpsc::channel(20);
            let (event_sender, event_receiver) = tokio::sync::broadcast::channel::<HelloWorldActorEvent>(20);
            let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel::<()>();
            let (task_sender, mut task_receiver) = tokio::sync::mpsc::channel::<Task<HelloWorldActor>>(20);
            tokio::spawn(async move {
                let mut actor = self;
                actor.start(task_sender, event_sender).await;
                tokio::select! { 
                    _ = actor.select_receivers (& mut msg_receiver , & mut task_receiver) => { log::debug!("(abcgen) all proxies dropped") ; } 
                    _ = stop_receiver => { log::debug!("(abcgen) stop signal received") ; } }
                actor.shutdown().await;
            });
            let proxy = HelloWorldActorProxy {
                message_sender: msg_sender,
                stop_signal: Some(stop_sender),
                events: event_receiver,
            };
            proxy
        }
        async fn select_receivers(
            &mut self,
            msg_receiver: &mut tokio::sync::mpsc::Receiver<HelloWorldActorMessage>,
            task_receiver: &mut tokio::sync::mpsc::Receiver<Task<HelloWorldActor>>,
        ) {
            loop {
                tokio::select! { 
                    msg = msg_receiver.recv () => {
                        match msg { 
                            Some (msg) => { self.dispatch(msg).await; }
                            None => { break ; } 
                        } 
                    }, 
                    task = task_receiver.recv () => { 
                        if let Some (task) = task {
                            task (self) . await ; 
                        } 
                    } 
                }
            }
        }
        async fn dispatch(&mut self, message: HelloWorldActorMessage) {
            match message {
                HelloWorldActorMessage::TellMeYourName { caller, respond_to } => {
                    let result = self.tell_me_your_name(caller).await;
                    respond_to.send(result).unwrap();
                }
            }
        }
        fn invoke(
            sender: &tokio::sync::mpsc::Sender<Task<HelloWorldActor>>,
            task: Task<HelloWorldActor>,
        ) -> Result<(), AbcgenError> {
            sender
                .try_send(task)
                .map_err(|e| AbcgenError::ChannelError(Box::new(e)))
        }
    }
}

use hello_wordl_actor::{HelloWorldActor, HelloWorldActorEvent};
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
    let thename = proxy.tell_me_your_name("Alice".to_string()).await.unwrap();
    println!("The actor replied with name: \"{}\"", thename);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
}
