# GUI Dialect Plan

## Overview

The Phorensic OS GUI is not a port of an existing GUI system. It is a native capability-based compositing display server designed from first principles, using KDE Plasma / KWin / Qt as the richest GUI dialect surface to mine for semantic shape.

## GUI Architecture

```text
Compositor (Phorensic Compositor)
  ↔ Input Handler (capability-gated)
  ↔ Display Server (native protocol)
  ↔ Window Manager (policy)
  ↔ Client Applications (capability-gated)
  ↔ Dialect Cage (Wayland/X11 compatibility for foreign clients)
```

### Native Protocol

The native display protocol is capability-based:

```text
PhorensicDisplayProtocol:
  - capabilities: create_surface, present, receive_input, configure
  - surfaces: typed (window, overlay, cursor, popup)
  - presentation: explicit, with present receipts
  - input: capability-gated event channels
  - configuration: atomic surface state transactions
```

### Compositor

The compositor (Phorensic Compositor) manages:

- surface creation and destruction
- compositing (hardware-accelerated where available, software fallback)
- presentation scheduling
- input routing (capability-gated)
- output management (display hotplug, mode setting)

### Window Manager

The window manager is a separate component with specific capabilities:

```text
WindowManagerCapabilities:
  - enumerate_surfaces
  - configure_surface (position, size, state)
  - focus_surface (input focus)
  - manage_window_stack (z-order)
  - apply_decoration (client-server or server-side)
```

The window manager is not the compositor. They communicate through the native protocol with capability gating.

## GUI Dialect Mining from KDE Plasma

### Components to Mine

```text
KWin compositor:
  - compositing pipeline (OpenGL/Vulkan based)
  - window management (tiling, stacking, effects)
  - input handling (keyboard, pointer, touch)
  - output management (multi-monitor, hotplug)
  - scene graph (rendering abstraction)

Plasma Shell:
  - panel layout and management
  - widget/applet system
  - system tray
  - application launcher
  - notifications

KRunner:
  - query interface
  - plugin/runner system
  - result display

KInfoCenter:
  - hardware enumeration
  - device information display
  - capability reporting
```

### Semantic Shape Extraction

For each component, extract:

```text
Component: KWin compositing pipeline
Dialect: wayland-kwin-qt
Protocol messages:
  - wl_surface.commit (presentation)
  - wl_buffer.release (buffer lifecycle)
  - zwp_linux_dmabuf_v1 (buffer sharing)
  - xdg_surface.configure (window configuration)
State machine:
  - surface: initial → configured → mapped → unmapped → destroyed
  - window: created → configured → mapped → focused → unfocused → unmapped → destroyed
Capabilities used:
  - DRM master (display control)
  - evdev (input device access)
  - GPU (rendering)
Effects:
  - display_output
  - input_receive
  - gpu_render
  - memory_map (buffer sharing)
```

## Native GUI Component Design

### Compositor

```text
Compositor:
  - protocol handler (native Phorensic display protocol)
  - compositing engine (software first, hardware acceleration later)
  - presentation scheduler (vsync-aware, residual-recording)
  - output connector (framebuffer, DRM dialect cage for initial hardware support)

Compositing pipeline (software reference):
  1. Receive surface update from client
  2. Composite surface into framebuffer
  3. Present via framebuffer flip
  4. Record present receipt
  5. Send frame callback to client
```

### Input Handler

```text
InputHandler:
  - device capability grants (keyboard, pointer, touch)
  - event capture and routing
  - focus management (window manager collaboration)
  - input method support (text input)

Input flow:
  Device → Kernel (evdev dialect cage) → InputHandler → Compositor → FocusedClient
```

### Display Server

```text
DisplayServer:
  - protocol dispatcher (capability-verified message routing)
  - surface registry (all live surfaces)
  - output registry (all connected displays)
  - buffer management (shared memory, dmabuf)
  - session management (client connection lifecycle)
```

### Client GUI Toolkit

```text
PhorensicGUI:
  - surface creation (capability-gated)
  - rendering (software canvas, hardware acceleration planned)
  - input handling (event subscriptions)
  - window management (configure, move, resize, close)
  - widget system (buttons, labels, text, containers)
  - theme support (capability-sealed theme packages)

Initial toolkit components:
  - Window (top-level surface)
  - Label (text display)
  - Button (clickable)
  - Container (layout)
  - TextInput (keyboard input)
  - Canvas (custom rendering)
```

## GUI Stand-Ins

The mining process identifies these GUI stand-ins for initial Phorensic OS GUI:

```text
KWin-like compositor → Phorensic Compositor [FUTURE]
Plasma-like shell → Phorensic Shell [FUTURE]
KRunner-like command surface → Phorensic Runner [FUTURE]
KInfoCenter-like inspector → Phorensic Inspector [FUTURE]
System Settings-like capability policy → Phorensic Policy [FUTURE]
```

Each stand-in starts as a simplified native version and evolves through progressive mining and replacement.

## Dialect Cage: Wayland Compatibility

For foreign Wayland clients, a dialect cage provides compatibility:

```text
WaylandCage:
  - implements Wayland protocol (observed from reference compositor)
  - translates Wayland protocol messages to native Phorensic protocol
  - captures residuals for each translation
  - progressively develops native equivalent protocol features
```

This is not a permanent compatibility layer. The cage allows foreign clients to run while their functionality is being residualized and their protocol requirements are being understood for native replacement.

## Staged Implementation

### Stage 1: Framebuffer Output [FIRST]

```text
- Simple framebuffer compositor
- Direct framebuffer write
- No window management
- Capability: display_output
- Status: [CLAIMED]
```

### Stage 2: Simple Window [NEXT]

```text
- Window creation (single surface)
- Basic compositing (overlay)
- Input handling (keyboard, pointer)
- Capability: create_surface, receive_input
- Status: [FUTURE]
```

### Stage 3: Window Manager [NEXT]

```text
- Multiple window management
- Focus, stacking, move, resize
- Input routing
- Capability: manage_windows, focus_surface
- Status: [FUTURE]
```

### Stage 4: Shell and Desktop [LATER]

```text
- Desktop panel
- Application launcher
- System tray
- Notifications
- Status: [FUTURE]
```

### Stage 5: Full Native GUI [LATER]

```text
- Complete widget toolkit
- Hardware acceleration
- Compositor effects
- Accessibility
- Status: [FUTURE]
```

## Example: GUI Capability Flow

```text
// Client requests surface creation
let cap = session.grant_capability(Capability::SurfaceCreate)?
let surface = compositor.create_surface(cap, SurfaceType::Window)?

// Client renders to surface
let buf = surface.allocate_buffer(Width(800), Height(400))?
let canvas = Canvas::from_buffer(buf)
canvas.fill(Color::white())
canvas.draw_text(10, 10, "Hello Phorensic GUI")
surface.present(buf, PresentMode::Vsync)?

// Compositor composites and presents
compositor.on_surface_update(surface, |update| {
    let frame = compositor.composite_frame()?
    frame.add_surface(surface, Position(0, 0))
    frame.present()?
    // Present recorded as residual
    record_residual("present", frame.hash())
})

// Client receives frame callback
surface.on_frame(|timestamp, seq| {
    // Ready for next frame
    render_frame()
})
```
