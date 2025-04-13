use directories::BaseDirs;
use parking_lot::RwLock;
use std::{
    error::Error,
    sync::{atomic::AtomicBool, Arc},
    thread,
};
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{debug, Level};
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, registry::Registry, Layer};
use windows_registry_searcher::{
    controls::controls, renderer::renderer_wrappers_wrapper, static_selection::StaticSelection,
    worker_runtime::worker_runtime, Focus,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let base_directories = BaseDirs::new().expect("Base directories not found");
    let log_path = base_directories
        .config_dir()
        .join("windows_registry_search/logs/");
    let file = tracing_appender::rolling::daily(log_path, format!("log"));
    let (file_writer, _guard) = tracing_appender::non_blocking(file);
    let level_filter = LevelFilter::from_level(Level::DEBUG);
    let logfile_layer = tracing_subscriber::fmt::layer()
        .with_line_number(true)
        .with_writer(file_writer)
        .with_filter(level_filter);
    let subscriber = Registry::default().with(logfile_layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let (tx, rx) = mpsc::channel::<()>(1);

    let focus: Arc<RwLock<Focus>> = Arc::new(RwLock::new(Focus::Main));
    let static_menu_selection: Arc<StaticSelection> = Arc::new(StaticSelection::default());
    let stop: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let static_menu_selection_ = static_menu_selection.to_owned();
    let focus_ = focus.to_owned();
    let stop_ = stop.to_owned();
    let controls_thread = thread::spawn(move || {
        controls(static_menu_selection_, focus_, stop_, tx);
        debug!("Controls thread closed");
    });

    let stop_: Arc<AtomicBool> = stop.to_owned();
    let static_menu_selection_ = static_menu_selection.to_owned();
    let focus_ = focus.to_owned();
    let renderer_thread = thread::spawn(move || {
        let _ = renderer_wrappers_wrapper(static_menu_selection_, focus_, stop_);
        debug!("Renderer thread closed");
    });
    let worker_thread: JoinHandle<()> =
        tokio::spawn(worker_runtime(static_menu_selection, rx, stop.to_owned()));

    let _ = renderer_thread.join();
    let _ = controls_thread.join();
    let _ = worker_thread.await;
    Ok(())
}
