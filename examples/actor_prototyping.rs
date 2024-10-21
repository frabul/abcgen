//! A sandbox example that i used to prototype what the code generator should finally produce. 
//! This file is not maintained and could eventually differ from the final implementation.

#![allow(unused)]

use my_actor_module::{MyActor, MyActorEvent};
use std::sync::{atomic::AtomicBool, Arc};
//#[actor_module]
mod my_actor_module {

    use std::{
        future::Future,
        pin::Pin,
        sync::{atomic::AtomicBool, Arc},
    };

    pub type PinnedFuture<'a, TResult> = Pin<Box<dyn Future<Output = TResult> + Send + 'a>>;

    pub type Task<TActor> = Box<dyn (for<'b> Fn(&'b mut TActor) -> PinnedFuture<'b, ()>) + Send>;

    pub type TaskSender = tokio::sync::mpsc::Sender<Task<MyActor>>;
    pub type EventSender = tokio::sync::broadcast::Sender<MyActorEvent>;
    #[derive(Debug, thiserror::Error)]
    pub enum AbcgenError {
        #[error("Service is already stopped")]
        AlreadyStopped,
        #[error("Failed to send message. Channel overrun or service stopped.")]
        ChannelError(#[source] Box<dyn std::error::Error>),
    }
    #[derive(Debug, Clone)]
    pub enum MyActorEvent {
        Event1,
        Event2,
    }

    pub struct MyActor {
        pub(crate) termination_requested: Arc<AtomicBool>,
        pub(crate) internal_task: Option<tokio::task::JoinHandle<()>>,
    }

    impl MyActor {
        pub async fn start(
            &mut self,
            task_sender: tokio::sync::mpsc::Sender<Task<MyActor>>,
            event_sender: tokio::sync::broadcast::Sender<MyActorEvent>,
        ) {
            log::info!("Starting");
            let term_req = self.termination_requested.clone();
            let internal_task = tokio::spawn(async move {
                Self::invoke_fn(&task_sender, Self::dummy_task_2).unwrap();
                Self::invoke(
                    &task_sender,
                    Box::new(|runnable| {
                        Box::pin(async {
                            log::info!("Executing a closure task");
                            runnable.dummy_task().await;
                        })
                    }),
                )
                .unwrap();
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

        async fn do_this(&mut self, par1: i32, par2: String) {
            log::info!("do_this called: par1={}, par2={}", par1, par2);
        }

        async fn do_that(&mut self) {
            log::info!("do_that called");
        }

        async fn get_that(&mut self, name: String) -> i32 {
            log::info!("get_that called: name={}", name);
            42
        }
    }
    
    #[derive(Debug)]
    pub enum MyActorMessage {
        DoThis {
            par1: i32,
            par2: String,
        },
        DoThat {},
        GetThat {
            name: String,
            respond_to: tokio::sync::oneshot::Sender<i32>,
        },
    }
    
    pub struct MyActorProxy {
        message_sender: tokio::sync::mpsc::Sender<MyActorMessage>,
        events: tokio::sync::broadcast::Receiver<MyActorEvent>,
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
    
        pub fn get_events(&self) -> tokio::sync::broadcast::Receiver<MyActorEvent> {
            self.events.resubscribe()
        }

        pub async fn do_this(&self, par1: i32, par2: String) -> Result<(), AbcgenError> {
            let msg = MyActorMessage::DoThis { par1, par2 };
            let send_res = self
                .message_sender
                .send(msg)
                .await
                .map_err(|e| AbcgenError::ChannelError(Box::new(e)));
            send_res
        }

        pub async fn do_that(&self) -> Result<(), AbcgenError> {
            let msg = MyActorMessage::DoThat {};
            let send_res = self
                .message_sender
                .send(msg)
                .await
                .map_err(|e| AbcgenError::ChannelError(Box::new(e)));
            send_res
        }

        pub async fn get_that(&self, name: String) -> Result<i32, AbcgenError> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let msg = MyActorMessage::GetThat {
                name,
                respond_to: tx,
            };
            let send_res = self.message_sender.send(msg).await;
            match send_res {
                Ok(_) => rx.await.map_err(|e| AbcgenError::ChannelError(Box::new(e))),
                Err(e) => Err(AbcgenError::ChannelError(Box::new(e))),
            }
        }
    }

    impl MyActor {
        pub fn run(self) -> MyActorProxy {
            let (msg_sender, mut msg_receiver) = tokio::sync::mpsc::channel(20);
            let (event_sender, event_receiver) =
                tokio::sync::broadcast::channel::<MyActorEvent>(20);
            let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel::<()>();
            let (task_sender, mut task_receiver) = tokio::sync::mpsc::channel::<Task<MyActor>>(20);
            tokio::spawn(async move {
                let mut actor = self;
                actor.start(task_sender, event_sender).await;
                tokio::select! { _ = actor . select_receivers (& mut msg_receiver , & mut task_receiver) => { log :: debug ! ("(abcgen) all proxies dropped") ; } _ = stop_receiver => { log :: debug ! ("(abcgen) stop signal received") ; } }
                actor.shutdown().await;
            });
            let proxy = MyActorProxy {
                message_sender: msg_sender,
                stop_signal: Some(stop_sender),
                events: event_receiver,
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
                MyActorMessage::DoThat {} => {
                    self.do_that().await;
                }
                MyActorMessage::GetThat { name, respond_to } => {
                    let result = self.get_that(name).await;
                    respond_to.send(result).unwrap();
                }
            }
        }

        fn invoke(
            sender: &tokio::sync::mpsc::Sender<Task<MyActor>>,
            task: Task<MyActor>,
        ) -> Result<(), AbcgenError> {
            sender
                .try_send(task)
                .map_err(|e| AbcgenError::ChannelError(Box::new(e)))
        }

        fn invoke_fn(
            sender: &tokio::sync::mpsc::Sender<Task<MyActor>>,
            task: fn(&mut MyActor) -> PinnedFuture<()>,
        ) -> Result<(), AbcgenError> {
            let task: Task<MyActor> = Box::new(task);
            sender
                .try_send(task)
                .map_err(|e| AbcgenError::ChannelError(Box::new(e)))
        }

    }
}


// ----------------- main -----------------
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
