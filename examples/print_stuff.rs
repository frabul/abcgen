//! Illustrates the functionalities by printing messages to the console.

#![allow(unused)]
use my_actor_module::{MyActor, MyActorEvent};
use std::sync::{atomic::AtomicBool, Arc};

use abcgen::actor_module;
#[actor_module]
mod my_actor_module {
    use std::sync::{atomic::AtomicBool, Arc};

    use abcgen::*;
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
        // following functions must be implemented by the user
        pub async fn start(&mut self, task_sender: TaskSender, event_sender: EventSender) {
            log::info!("Starting");
            let term_req = self.termination_requested.clone();
            let internal_task = tokio::spawn(async move {
                send_task!( task_sender(this) => { this.dummy_task().await; } );
                send_task!( task_sender(this) => {
                    log::info!("Executing a closure task");
                    this.dummy_task().await;
                } );

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

        //pub async fn dummy_task(&mut self) {
        pub async fn dummy_task<'b>(&'b mut self) {
            log::info!("Dummy task executed");
        }

        pub fn dummy_task_2(&mut self) -> PinnedFuture<'_, ()> {
            Box::pin(async {
                log::info!("Dummy dummy task 2 executed");
                self.dummy_task().await;
            })
        }

        #[message_handler] // the methods marked with this attribute will have a counterpart on the proxy
        async fn do_this(&mut self, par1: i32, par2: String) {
            log::info!("do_this called: par1={}, par2={}", par1, par2);
        }

        #[message_handler]
        async fn do_that(&mut self) -> Result<(), ()> {
            log::info!("do_that called");
            Ok(())
        }

        #[message_handler] // the can also have a return value
        async fn get_that(&mut self, name: String) -> i32 {
            log::info!("get_that called: name={}", name);
            42
        }
    } // impl MyActor
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
