use anyhow::{anyhow, Result};
use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{
    fmt,
    fmt::{
        format::{Format, Pretty},
        Subscriber,
    },
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer as LayerT,
};

type DefaultSubscriber = Subscriber<Pretty, Format<Pretty>, EnvFilter>;

pub fn init_log(run_name: String) -> Result<WorkerGuard> {
    let (guard, file_layer) = create_file_log_component(run_name);

    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter("gear_node_loader=debug,gclient=debug,gsdk=debug,gear_program=debug,gear_call_gen=debug")
        .with_line_number(false)
        .with_file(false)
        .with_target(false)
        .with_ansi(false)
        .finish()
        .with(file_layer)
        .try_init()
        .map(|_| guard)
        .map_err(|_| anyhow!("Can't initialize logger"))
}

fn create_file_log_component(run_name: String) -> (WorkerGuard, impl LayerT<DefaultSubscriber>) {
    let file_name_prefix = format!("{run_name}-loader");
    let file_appender = rolling::hourly("./log", file_name_prefix);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let layer = fmt::layer()
        .pretty()
        .with_ansi(false)
        .with_writer(non_blocking);
    (guard, layer)
}
