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
Exec=dlk-objviewer -settings settings.json %F
Icon-Name=applications-graphics
Selection=s
Extensions=obj;
```

If your `.obj` files use a different coordinate system convention, you may want to modify the flags:

```
-f +z -u +y -r +x
```

to match your coordinate system. 
If you don't want to derive missing normals just set --derive-normals to false. 
If you want to set the mouse sensitivity just set --mouse-sensitivity to whatever you prefer. 

---

## Settings

### Bindings
Bindings are defined in the settings file under the `"bindings"` section. Each binding consists of the following fields:

- **`command`**  
  The command that will be executed.  
  Examples: `move_forward`, `move_left`

- **`input`**  
  The key or button that activates the command.  
  Examples: `w`, `a`, `s`, `d`

- **`event`**  
  Specifies when the command will be executed.  
  Examples: `press`, `hold`, `release`

- **`requires`** *(optional)*  
  Defines an additional requirement that must be met for the command to execute. This typically references another binding.

#### Example
To require **Shift + Click** to move the camera forward:

1. Add a condition to the `move_forward` binding:
   ```yaml
   "requires": "move_forward_requirement"
---

## License

This project is licensed under the **Apache 2.0 License**.

---

## Notes

This repository is primarily a learning project.  
Expect frequent changes as new graphics programming concepts are explored.
