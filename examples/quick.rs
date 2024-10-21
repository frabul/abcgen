#[abcgen::actor_module]
#[allow(unused)]
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
