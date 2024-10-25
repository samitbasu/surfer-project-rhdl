// Web apps which integrate Surfer as an iframe can give commands to surfer via
// the .postMessage [1] function on the iframe.
//
//  For example, to tell Surfer to load waveforms from a URL, use
// `.postMessage({command: "LoadUrl", url: "https://app.surfer-project.org/picorv32.vcd"})`
//
// We have some special case functions
// for common use cases, for example `LoadUrl` for which we will keep the API stable
// as far as possible.
// For more complex functionality, one can also inject any `Message` defined in `surfer::Message` in surfer/main.rs. However, the API of these messages is not stable and may change at any time. If you add functionality via these, make sure to test the new functionality when changing Surfer version.
//
// [1] https://developer.mozilla.org/en-US/docs/Web/API/Window/postMessage

function register_message_listener() {
  window.addEventListener("message", (event) => {
    // JSON decode the message
    const decoded = event.data

    switch (decoded.command) {
      // Load a waveform from a URL. The format is inferred from the data.
      // Example: `{command: "LoadUrl", url: "https://app.surfer-project.org/picorv32.vcd"}`

      case 'LoadUrl': {
        const msg = {
          Stable: {LoadFromUrl: decoded.url}
        }
        inject_message(JSON.stringify(msg))
        break;
      }

      case 'ToggleMenuBar': {
        const msg = {Stable: "ToggleMenuBar"}
        inject_message(JSON.stringify(msg))
        break;
      }

      case 'HideMenuBar': {
        const msg = {stable: "HideMenuBar"}
        inject_message(JSON.stringify(msg))
        break;
      }

      // Inject any other message supported by Surfer in the surfer::Message enum.
      // NOTE: The API of these is unstable.
      case 'InjectMessage': {
        inject_message(decoded.message);
        break
      }

      default:
        console.log(`Unknown message.command ${decoded.command}`)
        break;
    }
  });
}
