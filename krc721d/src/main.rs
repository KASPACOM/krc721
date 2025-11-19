pub const GIT_DESCRIBE: &str = env!("VERGEN_GIT_DESCRIBE");
pub const GIT_SHA: &str = env!("VERGEN_GIT_SHA");
pub const RUSTC_CHANNEL: &str = env!("VERGEN_RUSTC_CHANNEL");
pub const RUSTC_COMMIT_DATE: &str = env!("VERGEN_RUSTC_COMMIT_DATE");
pub const RUSTC_COMMIT_HASH: &str = env!("VERGEN_RUSTC_COMMIT_HASH");
pub const RUSTC_HOST_TRIPLE: &str = env!("VERGEN_RUSTC_HOST_TRIPLE");
pub const RUSTC_LLVM_VERSION: &str = env!("VERGEN_RUSTC_LLVM_VERSION");
pub const RUSTC_SEMVER: &str = env!("VERGEN_RUSTC_SEMVER");
pub const CARGO_TARGET_TRIPLE: &str = env!("VERGEN_CARGO_TARGET_TRIPLE");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        fn main() {}
    } else {
        pub mod imports;
        pub mod error;
        pub mod result;

        pub mod args;
        pub mod config;
        pub mod logs;
        pub mod panic;
        pub mod folders;
        pub mod krc721d;

        use krc721d::Server;

        use mimalloc::MiMalloc;
        #[global_allocator]
        static GLOBAL: MiMalloc = MiMalloc;

        #[tokio::main]
        async fn main() {
            match Server::default().run().await {
                Ok(_) => println!("bye!"),
                Err(err) => eprintln!("Error: {err}"),
            }
        }
    }
}
