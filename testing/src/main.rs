pub const VERSION: &str = env!("CARGO_PKG_VERSION");

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        fn main() {}
    } else {
        pub mod imports;
        pub mod error;
        pub mod result;

        pub mod args;
        pub mod logs;
        pub mod recorder;
        pub mod player;
        pub mod testing;
        pub mod database;
        pub mod procload;

        use tracing::{info, error};
        use testing::Server;


        #[tokio::main]
        async fn main() {

            match Server::default().run().await {
                Ok(_) => info!("bye!"),
                Err(err) => error!("Error: {err}"),
            }
        }
    }
}
