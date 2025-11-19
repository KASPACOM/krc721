cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        pub mod error;
        pub mod imports;
        pub mod limits;
        pub mod result;
        pub mod service;
        pub mod path;
        pub mod metrics;
        pub mod filter;

        pub use service::*;
    }
}
