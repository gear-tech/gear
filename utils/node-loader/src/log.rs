use anyhow::{anyhow, Result};
use std::io::{self, Stdout};
use tracing_appender::{
    non_blocking::{NonBlocking, WorkerGuard},
    rolling,
};
use tracing_subscriber::{
    fmt,
    fmt::{
        format::{DefaultFields, Format, Pretty},
        Layer, Subscriber,
    },
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer as LayerT,
};

type StdoutLayer = LogLayer<fn() -> Stdout>;
type FileLayer = LogLayer<NonBlocking>;
type LogLayer<O> = Layer<DefaultSubscriber, Pretty, Format<Pretty>, O>;
type DefaultSubscriber = Subscriber<DefaultFields, Format, EnvFilter>;

pub fn init_log() -> Result<WorkerGuard> {
    let (guard, file_layer) = create_file_log_component();
    let writers = create_stdout_log_component().and_then(file_layer);

    tracing_subscriber::fmt()
        .with_env_filter("gear_node_loader=debug,gear_program=debug")
        .finish()
        .with(writers)
        .try_init()
        .map(|_| guard)
        .map_err(|_| anyhow!("Can't initialize logger"))
}

fn create_file_log_component() -> (WorkerGuard, FileLayer) {
    let file_appender = rolling::hourly("./log", "loader");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let layer = fmt::layer()
        .pretty()
        .with_ansi(false)
        .with_writer(non_blocking);
    (guard, layer)
}

fn create_stdout_log_component() -> StdoutLayer {
    fmt::layer()
        .pretty()
        .with_ansi(false)
        .with_writer(io::stdout)
}
