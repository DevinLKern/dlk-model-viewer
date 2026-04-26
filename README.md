# dlk-model-viewer

Experiments for learning Vulkan, graphics programming, and game development.

This repository currently is a Wavefront OBJ model viewer.  
It currently builds an executable capable of loading and displaying `.obj` files.

This project is primarily intended as a sandbox for exploring graphics concepts and Vulkan workflows.

---

## Features

- Load and view **Wavefront `.obj` models**
- Lightweight viewer executable
- Built with Rust
- Uses Vulkan-based rendering

More features will be added as the project evolves.

---

## Repository Structure

```
sandbox/   Rust project containing the OBJ viewer
```

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

## Nemo File Manager Integration

You can add a **right-click action** in the Nemo file manager to open `.` files with `dlk-model-viewer`.

Create the following file:

```
~/.local/share/nemo/actions/view_obj.nemo_action
```

With the following contents:

```
[Nemo Action]
Name=View obj model
Comment=Open obj model with dlk-model-viewer
Exec=dlk-model-viewer --settings custom_settings.yaml %F
Icon-Name=applications-graphics
Selection=s
Extensions=obj;
```
 

---

## Settings

See SETTINGS.md

## License

This project is licensed under the **Apache 2.0 License**.

---

## Notes

This repository is primarily a learning project.  
Expect frequent changes as new graphics programming concepts are explored.
