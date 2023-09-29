# Surfer

A VCD viewer with a focus on a snappy usable interface, and extensibility

![A screenshot of surfer](misc/screenshot.png)

## Installation

### Dependencies
 - [Rust via Rustup](https://rustup.rs)
 - libssl

    Ubuntu:
    ```bash
    $sudo apt install openssl
    $sudo apt install libssl-dev
    ```

    Fedora:
    ```bash
    $sudo dnf update
    $sudo dnf install openssl
    $sudo dnf install openssl-devel
    ```
### Compiling
```bash
cargo install --git https://gitlab.com/surfer-project/surfer surfer
```

## Project Status

Surfer is still in early development, but it is in a usable state. In fact, if
you are able to take advantage of the extensibility such as with the
[Spade](https://spade-lang.org) integration, you might even prefer it to the alternatives.

As an indication of the status of the project, here is an incomplete list of supported and planned features

- [x] Basic rendering of waves
- [x] Snappy zooming, panning and general interaction
- [x] VCD loading
- [ ] FST loading
- [x] [Fuzzy completion based command line interface](misc/surfer_ui_trimmed.mp4)
- [ ] Bit translation
    - [x] Raw bits
    - [x] Hexadecimal values
    - [x] Unsigned values
    - [x] [Spade](https://spade-lang.org) values
    - [x] Signed values
    - [x] Octal values
    - [x] VHDL nine-valued std_ulogic support
    - [x] ASCII
    - [ ] RiscV instructions (probably via an extension)
    - [ ] Custom translation via Python API
- [ ] Wave file reloading
- [ ] Saving and Loading selected waves
- [ ] Cursors for measuring time
- [ ] [WAL](https://wal-lang.org) integration

## License

Surfer is licensed under the [EUPL-1.2 license](LICENSE.txt).
