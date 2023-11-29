use futures_core::Future;
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

// NOTE: wasm32 does not require a Send bound.
#[cfg(target_arch = "wasm32")]
pub fn perform_async_work<F>(f: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(async {
        info!("Starting async task");
        f.await;
    });
    info!("Returning from perform work")
}

// NOTE: not wasm32 requires a Send bound too.
#[cfg(not(target_arch = "wasm32"))]
pub fn perform_async_work<F>(f: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async {
        info!("Starting async task");
        f.await;
    });
    info!("Returning from perform work")
}

pub struct UrlArgs {
    pub load_url: Option<String>,
    pub startup_commands: Option<String>,
}

#[cfg(target_arch = "wasm32")]
pub fn vcd_from_url() -> UrlArgs {
    let search_params = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|l| web_sys::UrlSearchParams::new_with_str(&l).ok());

    UrlArgs {
        load_url: search_params.as_ref().and_then(|p| p.get("load_url")),
        startup_commands: search_params
            .as_ref()
            .and_then(|p| p.get("startup_commands")),
    }
}
