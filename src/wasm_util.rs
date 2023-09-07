use log::info;

// Wasm doesn't seem to support std::thread, so this spawns a thread where we can
// but runs the work sequentially where we can not.
pub fn perform_work<F>(f: F)
where
    F: FnOnce() -> () + Send + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        wasm_bindgen_futures::spawn_local(async {
            info!("Starting async task");
            f();
        });
        info!("Returning from perform work")
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::spawn(async {
            info!("Starting async task");
            f()
        });
        info!("Returning from perform work")
    }
}
