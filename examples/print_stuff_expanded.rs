#![doc = " Illustrates the functionalities by printing messages to the console."]
#![allow(unused)]
use abcgen::actor_module;
use my_actor_module::{MyActor, MyActorEvent};
use std::sync::{atomic::AtomicBool, Arc};
mod my_actor_module {
    use abcgen::*;
    use std::sync::{atomic::AtomicBool, Arc};
    #[events]
    #[derive(Debug, Clone)]
    pub enum MyActorEvent {
        Event1,
        Event2,
    }
    #[actor]
    pub struct MyActor {
        pub(crate) termination_requested: Arc<AtomicBool>,
        pub(crate) internal_task: Option<tokio::task::JoinHandle<()>>,
    }
    impl MyActor {
        pub async fn start(&mut self, task_sender: TaskSender, event_sender: EventSender) {
            log::info!("Starting");
            let term_req = self.termination_requested.clone();
            let internal_task = tokio::spawn(async move {
                send_task ! (task_sender (this) => { this . dummy_task () . await ; });
                send_task ! (task_sender (this) => { log :: info ! ("Executing a closure task") ; this . dummy_task () . await ; });
                while !term_req.load(std::sync::atomic::Ordering::Relaxed) {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    log::info!("Sending ThisHappend");
                    let _ = event_sender.send(MyActorEvent::Event1);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    log::info!("Sending ThatHappend");
                    let _ = event_sender.send(MyActorEvent::Event2);
                }
                log::info!("Event sender closed");
            });
            self.internal_task = Some(internal_task);
        }
        pub async fn shutdown(&mut self) {
            log::info!("Shutting down");
            self.termination_requested
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        pub async fn dummy_task<'b>(&'b mut self) {
            log::info!("Dummy task executed");
        }
        pub fn dummy_task_2(&mut self) -> PinnedFuture<'_, ()> {
            Box::pin(async {
                log::info!("Dummy dummy task 2 executed");
                self.dummy_task().await;
            })
        }
        #[message_handler]
        async fn do_this(&mut self, par1: i32, par2: String) {
            log::info!("do_this called: par1={}, par2={}", par1, par2);
        }
        #[message_handler]
        async fn do_that(&mut self) -> Result<(), ()> {
            log::info!("do_that called");
            Ok(())
        }
        #[message_handler]
        async fn get_that(&mut self, name: String) -> i32 {
            log::info!("get_that called: name={}", name);
            42
        }
    }
    type EventSender = tokio::sync::broadcast::Sender<MyActorEvent>;
    pub type TaskSender = tokio::sync::mpsc::Sender<Task<MyActor>>;
    #[derive(Debug)]
    pub enum MyActorMessage {
        DoThis {
            par1: i32,
            par2: String,
        },
        DoThat {
            respond_to: tokio::sync::oneshot::Sender<Result<(), ()>>,
        },
        GetThat {
            name: String,
            respond_to: tokio::sync::oneshot::Sender<i32>,
        },
    }
    impl MyActor {
        pub fn run(self) -> MyActorProxy {
            let (msg_sender, mut msg_receiver) = tokio::sync::mpsc::channel(10usize);
            let (event_sender, _) = tokio::sync::broadcast::channel::<MyActorEvent>(10usize);
            let event_sender_clone = event_sender.clone();
            let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel::<()>();
            let (task_sender, mut task_receiver) =
                tokio::sync::mpsc::channel::<Task<MyActor>>(10usize);
            tokio::spawn(async move {
                let mut actor = self;
                actor.start(task_sender, event_sender_clone).await;
                tokio::select! { _ = actor . select_receivers (& mut msg_receiver , & mut task_receiver) => { log :: debug ! ("(abcgen) all proxies dropped") ; } _ = stop_receiver => { log :: debug ! ("(abcgen) stop signal received") ; } }
                actor.shutdown().await;
            });
            let proxy = MyActorProxy {
                message_sender: msg_sender,
                stop_signal: Some(stop_sender),
                events: event_sender,
            };
            proxy
        }
        async fn select_receivers(
            &mut self,
            msg_receiver: &mut tokio::sync::mpsc::Receiver<MyActorMessage>,
            task_receiver: &mut tokio::sync::mpsc::Receiver<Task<MyActor>>,
        ) {
            loop {
                tokio::select! { msg = msg_receiver . recv () => { match msg { Some (msg) => { self . dispatch (msg) . await ; } None => { break ; } } } , task = task_receiver . recv () => { if let Some (task) = task { task (self) . await ; } } }
            }
        }
        async fn dispatch(&mut self, message: MyActorMessage) {
            match message {
                MyActorMessage::DoThis { par1, par2 } => {
                    self.do_this(par1, par2).await;
                }
                MyActorMessage::DoThat { respond_to } => {
                    let result = self.do_that().await;
                    respond_to.send(result).unwrap();
                }
                MyActorMessage::GetThat { name, respond_to } => {
                    let result = self.get_that(name).await;
                    respond_to.send(result).unwrap();
                }
            }
        }
    }
    pub struct MyActorProxy {
        message_sender: tokio::sync::mpsc::Sender<MyActorMessage>,
        events: tokio::sync::broadcast::Sender<MyActorEvent>,
        stop_signal: std::option::Option<tokio::sync::oneshot::Sender<()>>,
    }
    impl MyActorProxy {
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
        pub fn get_events(&self) -> tokio::sync::broadcast::Receiver<MyActorEvent> {
            self.events.subscribe()
        }
        pub async fn do_this(&self, par1: i32, par2: String) -> Result<(), AbcgenError> {
            let msg = MyActorMessage::DoThis { par1, par2 };
            let send_res = self
                .message_sender
                .send(msg)
                .await
                .map_err(|e| AbcgenError::ActorShutDown);
            send_res
        }
        pub async fn do_that(&self) -> Result<Result<(), ()>, AbcgenError> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let msg = MyActorMessage::DoThat { respond_to: tx };
            let send_res = self.message_sender.send(msg).await;
            match send_res {
                Ok(_) => rx.await.map_err(|e| AbcgenError::ActorShutDown),
                Err(e) => Err(AbcgenError::ActorShutDown),
            }
        }
        pub async fn get_that(&self, name: String) -> Result<i32, AbcgenError> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let msg = MyActorMessage::GetThat {
                name,
                respond_to: tx,
            };
            let send_res = self.message_sender.send(msg).await;
            match send_res {
                Ok(_) => rx.await.map_err(|e| AbcgenError::ActorShutDown),
                Err(e) => Err(AbcgenError::ActorShutDown),
            }
        }
    }
}
#[tokio::main]
async fn main() {
    env_logger::builder()
        .format_timestamp_millis()
        .format_module_path(false)
        .filter_level(log::LevelFilter::Debug)
        .init();
    let actor: MyActor = MyActor {
        termination_requested: Arc::new(AtomicBool::new(false)),
        internal_task: None,
    };
    let mut proxy = actor.run();
    let mut events = proxy.get_events();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    log::info!("Event received: {:?}", event);
                }
                Err(e) => {
                    match e {
                        tokio::sync::broadcast::error::RecvError::Closed => {
                            log::info!("Event channel closed");
                            break;
                        }
                        _ => {}
                    }
                    log::error!("Error receiving event: {:?}", e);
                    break;
                }
            }
        }
    });
    let res = proxy.get_that("test".to_string()).await;
    log::info!("get_that result: {:#?}", res);
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    proxy.do_this(1, "test".to_string()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    proxy.do_that().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    proxy.stop_and_wait().await.unwrap();
    log::info!("Wait before terminating the application.");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    log::info!("Terminating the application.");
}
