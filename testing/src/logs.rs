use std::path::Path;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

pub fn init_logs<P: AsRef<Path>>(logs_dir: P) -> WorkerGuard {
    let file_appender = rolling_file::BasicRollingFileAppender::new(
        logs_dir.as_ref().join("krc721-testing.log"),
        rolling_file::RollingConditionBasic::new()
            .max_size(1024 * 1024)
            .hourly(),
        usize::MAX,
    )
    .unwrap();

    let (non_blocking_appender, guard) = tracing_appender::non_blocking(file_appender);
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_writer(non_blocking_appender)
        .with_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        );
    let stdout_subscriber = tracing_subscriber::fmt::layer().with_filter(
        EnvFilter::builder()
            .with_default_directive(LevelFilter::INFO.into())
            .from_env_lossy(),
    );

    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(stdout_subscriber)
        .init();

    guard
}
