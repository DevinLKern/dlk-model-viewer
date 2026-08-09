# dlk-model-viewer

Experiments for learning Vulkan, graphics programming, and game development.
This repository currently is a Wavefront OBJ model viewer.
It currently builds an executable capable of loading and displaying `.obj` files.
Expect frequent changes to the code.

---

## Dependencies

To build this project you will need:

1. **Cargo** (Rust package manager)
2. **glslc** (GLSL shader compiler)

### Install Cargo

Follow the instructions on the Rust website:

https://rust-lang.org/

### Install glslc

On Ubuntu or Debian:

```bash
sudo apt install glslc
```

---

## Building

From the repository root:

```bash
cargo build
```

---

## Installing (Ubuntu / Debian)

This project can be packaged using `cargo-deb`.

```bash
cd sandbox
cargo deb
```

If `cargo deb` is not installed:

```bash
cargo install cargo-deb
```

---

## Usage

Open an OBJ model:

```bash
dlk-model-viewer model.obj
```

By default, `~/.config/dlk-model-viewer/default_settings.yaml` is used for configuration. You can optionally specify a custom settings file:

```bash
dlk-model-viewer --settings custom_settings.yaml model.obj
```

If you do this, the program will search for custom_settings in ~/.config/dlk-model-viewer. Please note that default_settings.yaml will be created automatically when the program runs.

---

## Settings

See SETTINGS.md

## File Manager Integration

Coming soon

## License

This project is licensed under the **Apache 2.0 License**.

