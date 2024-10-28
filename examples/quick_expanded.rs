#[allow(unused)]
mod actor {
    use abcgen::{actor, message_handler, send_task, AbcgenError, PinnedFuture, Task};
    #[actor]
    pub struct MyActor {
        pub some_value: i32,
    }
    impl MyActor {
        pub async fn start(&mut self, task_sender: TaskSender) {
            println!("Starting");
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                send_task ! (task_sender (this) => { this . example_task () . await ; });
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
        async fn example_task(&mut self) {
            println!("Example task executed");
        }
    }
    pub type TaskSender = tokio::sync::mpsc::Sender<Task<MyActor>>;
    #[derive(Debug)]
    pub enum MyActorMessage {
        GetValue {
            name: &'static str,
            respond_to: tokio::sync::oneshot::Sender<Result<i32, &'static str>>,
        },
    }
    impl MyActor {
        pub fn run(self) -> MyActorProxy {
            let (msg_sender, mut msg_receiver) = tokio::sync::mpsc::channel(20usize);
            let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel::<()>();
            let (task_sender, mut task_receiver) =
                tokio::sync::mpsc::channel::<Task<MyActor>>(20usize);
            tokio::spawn(async move {
                let mut actor = self;
                actor.start(task_sender).await;
                tokio::select! { _ = actor . select_receivers (& mut msg_receiver , & mut task_receiver) => { log :: debug ! ("(abcgen) all proxies dropped") ; } _ = stop_receiver => { log :: debug ! ("(abcgen) stop signal received") ; } }
                actor.shutdown().await;
            });
            let proxy = MyActorProxy {
                message_sender: msg_sender,
                stop_signal: Some(stop_sender),
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
                MyActorMessage::GetValue { name, respond_to } => {
                    let result = self.get_value(name).await;
                    respond_to.send(result).unwrap();
                }
            }
        }
        #[doc = r" Helper function to send a task to be invoked in the actor loop"]
        fn invoke(
            sender: &tokio::sync::mpsc::Sender<Task<MyActor>>,
            task: Task<MyActor>,
        ) -> Result<(), AbcgenError> {
            sender
                .try_send(task)
                .map_err(|e| AbcgenError::ActorShutdown)
        }
    }
    pub struct MyActorProxy {
        message_sender: tokio::sync::mpsc::Sender<MyActorMessage>,
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
                Some(tx) => tx.send(()).map_err(|_e: ()| AbcgenError::ActorShutdown),
                None => Err(AbcgenError::ActorShutdown),
            }
        }
        pub async fn stop_and_wait(&mut self) -> Result<(), AbcgenError> {
            self.stop()?;
            while self.is_running() {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Ok(())
        }
        pub async fn get_value(
            &self,
            name: &'static str,
        ) -> Result<Result<i32, &'static str>, AbcgenError> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let msg = MyActorMessage::GetValue {
                name,
                respond_to: tx,
            };
            let send_res = self.message_sender.send(msg).await;
            match send_res {
                Ok(_) => rx.await.map_err(|e| AbcgenError::ActorShutdown),
                Err(e) => Err(AbcgenError::ActorShutdown),
            }
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
