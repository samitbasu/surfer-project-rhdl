extern crate wasm_bindgen;
use wasm_bindgen::prelude::*;

use std::panic;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch)]
    fn on_surfer_error(msg: String) -> Result<(), JsValue>;

    type Error;

    #[wasm_bindgen(constructor)]
    fn new() -> Error;

    #[wasm_bindgen(structural, method, getter)]
    fn stack(error: &Error) -> String;
}

#[cfg(target_arch = "wasm32")]
fn panic_hook(info: &panic::PanicHookInfo) {
    use log::warn;

    let mut msg = info.to_string();

    // Add the error stack to our message.
    //
    // This ensures that even if the `console` implementation doesn't
    // include stacks for `console.error`, the stack is still available
    // for the user. Additionally, Firefox's console tries to clean up
    // stack traces, and ruins Rust symbols in the process
    // (https://bugzilla.mozilla.org/show_bug.cgi?id=1519569) but since
    // it only touches the logged message's associated stack, and not
    // the message's contents, by including the stack in the message
    // contents we make sure it is available to the user.
    msg.push_str("\n\nStack:\n\n");
    let e = Error::new();
    let stack = e.stack();
    msg.push_str(&stack);

    // Safari's devtools, on the other hand, _do_ mess with logged
    // messages' contents, so we attempt to break their heuristics for
    // doing that by appending some whitespace.
    // https://github.com/rustwasm/console_error_panic_hook/issues/7
    msg.push_str("\n\n");

    // Finally, run the user provided hook
    if let Err(e) = on_surfer_error(msg) {
        warn!("Failed to run on_surfer_error\n{e:?}")
    };
}

/// Set the `console.error` panic hook the first time this is called. Subsequent
/// invocations do nothing.
#[inline]
#[cfg(target_arch = "wasm32")]
pub fn set_once() {
    use std::sync::Once;

    use log::info;
    static SET_HOOK: Once = Once::new();
    SET_HOOK.call_once(|| {
        info!("Hook set up");
        let old_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            old_hook(info);
            panic_hook(info)
        }));
    });
}
