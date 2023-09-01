#[cfg(not(target_arch = "wasm32"))]
use std::thread;

// Wasm doesn't seem to support std::thread, so this spawns a thread where we can
// but runs the work sequentially where we can not.
pub fn perform_work<F>(f: F)
where
    F: FnOnce() -> () + Send + 'static,
{
    #[cfg(not(target_arch = "wasm32"))]
    thread::spawn(f);

    #[cfg(target_arch = "wasm32")]
    f()
}
