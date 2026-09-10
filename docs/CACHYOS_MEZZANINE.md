# CachyOS Mezzanine and Golden Path

## Overview

CachyOS serves as a model mezzanine — not the base of Phorensic OS, but a rich dialect-mining environment from which to acquire the residual surfaces needed to roll our own.

**Do not inherit CachyOS architecture. Mine it.**

## The Mezzanine Concept

A mezzanine is an intermediate platform used for observation and dialect mining. CachyOS provides a working, observable reference for multiple dialect surfaces:

- Linux kernel dialect (device drivers, syscalls, procfs/sysfs)
- systemd dialect (service management, session tracking)
- udev dialect (device discovery and management)
- DRM/KMS dialect (display and graphics)
- Mesa dialect (GPU userspace)
- Wayland dialect (compositor protocol)
- PipeWire dialect (audio/video routing)
- KDE/Plasma dialect (desktop environment, GUI toolkit)
- pacman/makepkg dialect (package management, build recipes)
- Arch Linux packaging dialect (PKGBUILDs, AUR)

Each of these is a dialect surface for observation, residual extraction, and eventual native clean-room replacement.

## Golden Path First

Start with one golden path through the mezzanine:

```text
CachyOS boots
→ KDE/Wayland session starts
→ input works (keyboard, mouse)
→ window opens (KWin compositor presents)
→ framebuffer/compositor renders
→ package runs (application launches)
→ build compiles (makepkg runs)
→ binary executes
→ residual trace captured
```

This path touches every major subsystem. Once this path is traced, residualized, and understood, the Phorensic OS team has the semantic shape of the entire system.

## Mining Methodology

### Step 1: Trace the Path

Walk the golden path on CachyOS, capturing residuals at every step:

```text
Tracing golden path on CachyOS:
1. Boot: capture kernel boot messages, module loading, init process
2. Session start: capture display manager, login, KDE Plasma startup
3. KWin compositor: capture window creation, compositing, presentation
4. Input: capture keyboard/mouse events through evdev → libinput → Wayland → KWin
5. GUI rendering: capture Qt widget rendering → KWin → Mesa → DRM → KMS
6. Package execution: capture binary loading, library resolution, execution
7. Build: capture makepkg execution, compilation, packaging
8. Binary execution: capture binary format handling, loader behavior
```

### Step 2: Classify Each Component

Each observed file and component receives a role classification:

```text
file: /usr/bin/kwin_x11
role: compositor
dialect: kde-wayland
observed_behavior: window management, compositing, input routing
dependencies: Mesa, DRM, libinput, Qt, Wayland
build_traces: CMake, gcc/mold, Qt build system
runtime_traces: Wayland protocol messages, DRM ioctls, Mesa EGL calls
residual_signature: hash(runtime_traces + build_traces + configuration)
```

### Step 3: Extract Residual Fingerprint

Each component produces a residual fingerprint:

```text
ResidualFingerprint =
  component_hash: Hash256
  dialect_profile: String
  observed_syscalls: list of (syscall_number, frequency, data_pattern)
  observed_ipc: list of (protocol, message_types, frequency)
  observed_file_access: list of (path_pattern, access_type, frequency)
  observed_capability_use: list of (capability, usage_pattern)
  build_configuration: BuildRecipe
  dependency_graph: list of (component, version, connection_type)
  timing_profile: (cpu_usage, memory_usage, io_profile)
```

### Step 4: Create Phorensic Spec Forensic Package

For each mined component, create a spec forensic package:

```text
.phor-spec package for mined component:
├── source evidence (from CachyOS package source)
├── signed source snapshot
├── source hash
├── behavior notes (observed runtime behavior)
├── dialect classification (which dialect surfaces it uses)
├── required capabilities (what it needs from the OS)
├── effects (IO, display, input, etc.)
├── expected residuals (what residuals it produces)
├── oracle traces (captured from CachyOS execution)
├── replay commands (how to replay the observation)
├── promotion criteria (what must be true for native replacement)
├── native Phorensic replacement plan (approach for clean-room spec)
├── emitted binary (from CachyOS)
├── binary hash
├── source↔binary seal (from CachyOS package verification)
└── court verdicts (if any court sessions run)
```

### Step 5: Replace Progressively

Replace each mined component with a native Phorensic OS equivalent:

```text
1. Understand the component's semantics (from mining)
2. Design native Phorensic specification
3. Implement native version
4. Verify against CachyOS oracle traces
5. Court-verify the replacement
6. Seal and promote the native version
7. Remove dependency on CachyOS dialect cage
```

## Dialect Surfaces to Mine

### Linux Kernel Dialect

```text
Surface: syscalls, ioctls, /proc, /sys, /dev, module interface
Key Observations:
  - syscall table (which calls, arguments, return values)
  - ioctl interface for DRM/KMS
  - device file semantics
  - module loading/unloading
  - interrupt handling model
  - memory management (mmap, brk)
  - process management (clone, exec, exit)

Mining Outputs:
  - syscall residual atlas
  - ioctl protocol specifications
  - device driver interface specifications
  - Phorensic capability mapping for each linux syscall pattern
```

### systemd Dialect

```text
Surface: service files, journal, logind, timedated, hostnamed
Key Observations:
  - unit file format and semantics
  - service dependency resolution
  - cgroup usage patterns
  - socket activation model
  - log/journal protocol

Mining Outputs:
  - service specification residual atlas
  - dependency resolution algorithm specification
  - Phorensic service model design
```

### DRM/KMS Dialect

```text
Surface: /dev/dri/card*, DRM ioctls, KMS properties, framebuffer
Key Observations:
  - DRM ioctl set and semantics
  - connector/encoder/crtc/plane model
  - framebuffer creation and presentation
  - mode setting sequence
  - page flip and vblank synchronization

Mining Outputs:
  - DRM protocol residual atlas
  - KMS mode setting specification
  - Phorensic display capability design
```

### Wayland Dialect

```text
Surface: wayland protocol, compositor interface, shm, xdg-shell
Key Observations:
  - protocol XML specifications
  - message serialization
  - event handling model
  - buffer sharing (dmabuf, shm)
  - xdg-shell surface lifecycle
  - input protocol (keyboard, pointer, touch)

Mining Outputs:
  - Wayland protocol residual atlas
  - compositor interface specification
  - Phorensic display server protocol design
```

### KDE/Plasma Dialect

```text
Surface: KWin, Plasma shell, KRunner, KInfoCenter, System Settings
Key Observations:
  - KWin compositing pipeline
  - Plasma shell panel/applet model
  - KRunner query/result protocol
  - KInfoCenter hardware enumeration
  - System Settings configuration model

Mining Outputs:
  - desktop environment component atlas
  - compositor pipeline specification
  - Phorensic GUI component design
```

### Mesa Dialect

```text
Surface: EGL, GLES, Vulkan, Gallium, Mesa state tracker
Key Observations:
  - EGL initialization and surface creation
  - GLES/Vulkan API surface
  - shader compilation and execution
  - buffer allocation and GPU memory management
  - GPU command submission

Mining Outputs:
  - GPU API residual atlas
  - shader compilation pipeline specification
  - Phorensic GPU capability design
```

### pacman/makepkg Dialect

```text
Surface: PKGBUILD format, pacman database, makepkg execution
Key Observations:
  - PKGBUILD variable and function semantics
  - dependency resolution
  - build step execution (fetch, prepare, build, package)
  - package database format
  - package installation script execution

Mining Outputs:
  - package recipe residual atlas
  - build pipeline specification
  - Phorensic package recipe format design
```

## Unit of Progress

File → Behavior → Residual → Spec → Court → Sealed Package

Every mined file follows this progression:

```text
1. File identified on CachyOS
2. Behavior observed and documented
3. Residual fingerprint extracted
4. Phorensic spec designed
5. Court session verifies spec against oracle traces
6. Sealed package created and stored
```

## Golden Path Milestone Criteria

The golden path is complete when:

- CachyOS boots with full residual tracing
- KDE/Wayland session produces residual fingerprint
- All major components in the path have role classifications
- Each component has a residual fingerprint
- Each component has a phor-spec forensic package (at least [CLAIMED] status)
- At least one component has a native Phorensic replacement ([IMPLEMENTED])
- The Phorensic OS can boot on reference hardware and display output through its own native compositor
- A simple application (terminal, file manager) can run natively

## Current Boot Progress (2026-07-04)

The following boot path milestones are now operational in the native Rust host runtime (`phost/`):

| Stage | Status | Component |
|-------|--------|-----------|
| Boot stub → GOP | ✅ | ASM → UEFI GOP framebuffer |
| Boot display | ✅ | StatusScreen (boot phases, progress bar, logs) |
| Graphics | ✅ | Canvas (shapes, text, compositing primitives) |
| Text console | ✅ | Scrolling console on framebuffer |
| Shell | ✅ | Interactive shell with 12 commands |
| Keyboard input | ✅ | PS/2 driver, scancode→ASCII |
| Serial I/O | ✅ | UART 16550 with capability model |
| `.phor` compilation | ✅ | 30/30 example files compile |
| Porting demo | ✅ | JIT/dialect cage + porting engine in .phor |

### Next Boot Targets
- Connect compiled `.phor` ELF objects to the Rust runtime
- Implement `.phor` GUI compositor (native, not Rust)
- Surface management and window manager
- Input routing from keyboard to compositor

## Non-Goals

- This is not a port of CachyOS to Phorensic.
- This is not a CachyOS compatibility layer.
- This is not about running CachyOS packages unmodified on Phorensic OS.
- This is not a Linux emulation system.

CachyOS is the quarry, not the foundation.
