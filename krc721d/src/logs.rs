use krc721_core::network::Network;
use std::path::Path;
use time::macros::format_description;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

pub fn init_logs<P: AsRef<Path>>(
    logs_dir: P,
    network: Network,
    level_filter: LevelFilter,
    _log_details: bool,
    suffix: Option<&str>,
) -> WorkerGuard {
    let file_name = if let Some(suffix) = suffix {
        format!("krc721d.{network}.{suffix}.log")
    } else {
        format!("krc721d.{network}.log")
    };

    let file_appender = rolling_file::BasicRollingFileAppender::new(
        logs_dir.as_ref().join(file_name),
        rolling_file::RollingConditionBasic::new()
            .max_size(1024 * 1024 * 8)
            .daily(),
        14,
    )
    .unwrap();

    let (non_blocking_appender, guard) = tracing_appender::non_blocking(file_appender);
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking_appender)
        .with_filter(
            EnvFilter::builder()
                .with_default_directive(level_filter.into())
                .from_env_lossy(),
        );

    let stdout_subscriber = tracing_subscriber::fmt::layer()
        .with_timer(tracing_subscriber::fmt::time::LocalTime::new(
            format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
        ))
        .with_filter(
            EnvFilter::builder()
                .with_default_directive(level_filter.into())
                .from_env_lossy(),
        );

    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(stdout_subscriber)
        .init();

    guard
}
