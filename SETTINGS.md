# Settings Configuration

This document describes the format of a `settings.yaml` file for the model viewer.

## Structure

A settings file consists of two top-level sections:

```yaml
bindings:
  # ... binding definitions

options:
  # ... option values
```

---

## Bindings

Each binding defines how user input triggers commands. Bindings have these fields:

| Field | Required | Description |
|-------|----------|-------------|
| `command` | Yes | The action to perform |
| `input` | Yes | The input that triggers the command |
| `event` | Yes | The event type that triggers the command |
| `requires` | No | Another command that must be active for this binding to work |

### Command

Valid command values:

- **Movement commands**: `move_forward`, `move_backward`, `move_left`, `move_right`, `move_up`, `move_down`
- **Camera commands**: `use_fps_camera`, `use_orbit_camera`, `zoom_in`, `zoom_out`
- **Rotation**: `rotate` (requires `mouse_moved` input with `movement` event - see note below)
- **Custom requirements**: Any custom string can be used as a command name for use with the `requires` field

### Input

Valid input values:

**Keys:**
- Letters: `a` - `z`
- Numbers: `0` - `9`
- Special: `space`, `shift_left`, `shift_right`, `ctrl_left`, `ctrl_right`, `=`, `-`

**Mouse:**
- `mouse1` (left button)
- `mouse2` (right button)
- `mouse_moved` (mouse movement)
- `mousewheel_up`
- `mousewheel_down`

### Event

Valid event values:

- `hold` - Command executes while the input is held down
- `toggle` - Command toggles on/off each time the input is triggered
- `press` - Command executes once when the input is pressed
- `release` - Command executes when the input is released
- `movement` - Only valid with `mouse_moved` input for rotation

### Requires

The `requires` field makes a binding conditional on another command being "active." When a binding with `requires` is triggered, it only works if the referenced command is currently active.

This is commonly used with a toggle command. For example, to require mouse button 1 to be held down for rotation to work:

```yaml
bindings:
  - command: rotate
    input: mouse_moved
    event: movement
    requires: mouse_move_requirement

  - command: mouse_move_requirement
    input: mouse1
    event: hold
```

### Important Note on Rotate

The `rotate` command has specific requirements:
- **Input must be**: `mouse_moved`
- **Event must be**: `movement`

Using any other input or event type with the `rotate` command will result in an error.

---

## Options

The `options` section contains global settings for the viewer.

| Setting | Type | Description |
|---------|------|-------------|
| `default_camera` | string | Camera mode on startup: `"fps"` or `"orbit"` |
| `fov_y` | float | Vertical field of view in degrees |
| `mouse_sensitivity` | float | Mouse sensitivity multiplier |
| `derive_normals` | boolean | Whether to derive normals if not present in model |

### default_camera

Sets the default camera mode. Valid values:
- `"fps"` - First-person camera with WASD movement
- `"orbit"` - Orbital camera that rotates around the model

### fov_y

The vertical field of view in degrees. Typical values range from 30 to 120.

### mouse_sensitivity

Controls how sensitive the mouse is for camera/rotation control. Higher values = more sensitive.

### derive_normals

When `true`, the viewer will compute surface normals if they are not present in the loaded model. This is useful for models without normal data but produces less accurate lighting than imported normals.

### model_right, model_up, and model_forward

These specify the coordinate system the model was made in. For example, if the model expects +y to be treated as the up direction, change model_up to +y, and set model_right and model_forward accordingly.

---

## Example

See files/default_settings.yaml
