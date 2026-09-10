# Driver Dialect Plan

## Overview

Drivers in Phorensic OS are capability-gated, court-verified, evidence-producing kernel extensions. They start constrained and gain privilege only through residual evidence and court verdicts.

**No driver is trusted by default.**

## Driver Architecture

```text
Driver =
  hardware_interface (MMIO, ports, IRQ, DMA)
  capability_gate (what the driver is allowed to do)
  evidence_log (residuals from driver operation)
  trust_state (current trust level)
  oracle_interface (reference behavior for verification)
  dialect_profile (which hardware dialect this driver implements)
```

### Driver Types

```text
DriverType = enum {
    SerialDebug     = 0x01,  // serial port, debug output
    Framebuffer     = 0x02,  // simple display output
    InputHID        = 0x03,  // human interface devices
    BlockDevice     = 0x04,  // storage devices
    TimerRTC        = 0x05,  // timing devices
    Watchdog        = 0x06,  // watchdog timer
    Network         = 0x07,  // network interface (later)
    GPUAccel        = 0x08,  // GPU acceleration (much later)
    PCIBus          = 0x09,  // PCI bus management
    ACPIControl     = 0x0A,  // ACPI (if required)
}
```

## Driver Order

The initial native driver work follows this priority:

```text
1. serial/debug        — essential for development and debugging
2. framebuffer         — essential for display output
3. input HID           — essential for user interaction
4. block device        — essential for forensic store storage
5. timer/RTC/watchdog  — essential for timing and system integrity
6. network             — needed for network communication (later)
7. GPU acceleration    — needed for hardware-accelerated GUI (much later)
```

Each driver type starts as a dialect cage over the Linux kernel driver (via the CachyOS mezzanine) and progressively transitions to a native Phorensic driver.

## Driver Lifecycle

```text
1. Hardware Discovery:
   - Nucleus enumerates hardware
   - Device identity established
   - Capability grants determined

2. Driver Loading:
   - Driver binary verified (integrity, signatures, trust state)
   - Capabilities granted (hardware access, memory, IRQ)
   - Driver loaded with initial trust state: Unknown

3. Driver Initiation:
   - Hardware initialized
   - Residuals collected
   - Behavior compared with oracle (if available)
   - Trust state may progress to Observed

4. Driver Operation:
   - Hardware operations performed within capability bounds
   - Residuals continuously collected
   - Trust state advances through evidence

5. Driver Promotion (if applicable):
   - Court session reviews driver evidence
   - If consistent and correct: trust promoted
   - Additional capabilities may be granted
   - Driver may transition from provisional to trusted

6. Driver Revocation (if applicable):
   - Residual drift detected
   - Hardware failure
   - Policy change
   - Capabilities revoked, driver constrained or terminated
```

## Driver Capability Model

Drivers receive hardware access capabilities:

```text
DriverCapabilities:
  mmio_access: region_list
  port_io_access: port_range_list
  irq_access: irq_number_list
  dma_access: allowed_memory_regions
  memory_claim: max_memory_bytes
  bus_master: bool (with bus type specification)
  power_control: bool
  reset_control: bool
```

Each capability is individually grantable and revocable.

## Driver Dialect Mining from CachyOS/Linux

### Serial/Debug Driver [FIRST]

```text
Mining Target: Linux serial driver (8250, UART)
Dialect Surface:
  - UART register interface (MMIO or port I/O)
  - interrupt handling (TX/RX ready)
  - baud rate, parity, stop bits configuration
  - FIFO management

Native Phorensic Serial Driver:
  - MMIO access to UART registers (capability-gated)
  - Interrupt handler for TX/RX
  - Fixed buffer (no heap)
  - Residual record for each byte transmitted
  - Trust starts at Unknown, promotes to Observed after first successful transmission
```

### Framebuffer Driver [NEXT]

```text
Mining Target: Linux simpledrm / vesafb
Dialect Surface:
  - Linear framebuffer (MMIO mapped)
  - Simple mode setting (resolution, depth)
  - Display timing (if configurable)

Native Phorensic Framebuffer Driver:
  - MMIO mapping to framebuffer (capability-gated)
  - Fixed resolution (or simple mode enumeration)
  - Pixel write (memory store)
  - Present via page flip or memcpy (no GPU)
  - Residual record for each present operation
```

### Input HID Driver [NEXT]

```text
Mining Target: Linux HID driver (hid-generic, usbhid)
Dialect Surface:
  - HID report descriptor parsing
  - Input event reporting (keyboard, mouse, touch)
  - USB HID transport

Native Phorensic Input Driver:
  - USB/PS2 access (capability-gated)
  - HID descriptor parsing (bounded, no heap)
  - Event emission (typed input events)
  - Capability-gated event channels to compositor
```

### Block Device Driver [NEXT]

```text
Mining Target: Linux NVMe driver, AHCI/SATA driver
Dialect Surface:
  - NVMe queue pair management
  - SATA command set
  - DMA operations

Native Phorensic Block Driver:
  - NVMe or AHCI register access (capability-gated)
  - DMA setup and teardown (capability-gated)
  - Fixed-size command queue (no heap)
  - Bounded I/O operations
  - Residual record for each I/O
```

### Timer/RTC/Watchdog Driver [NEXT]

```text
Mining Target: Linux timer, RTC, watchdog drivers
Dialect Surface:
  - HPET/APIC timer programming
  - CMOS RTC access
  - Watchdog timer programming

Native Phorensic Timer/RTC/Watchdog:
  - Timer register access (capability-gated)
  - RTC read/write (capability-gated)
  - Watchdog pet/configure (capability-gated)
  - Reliability-critical: line-audited
```

### Network Driver [LATER]

```text
Mining Target: Linux network drivers (e1000, virtio-net)
Dialect Surface:
  - DMA ring management
  - Packet buffer management (fixed pools)
  - Interrupt moderation

Native Phorensic Network Driver:
  - DMA access (capability-gated)
  - Fixed buffer pool (no heap)
  - Packet send/receive with residuals
  - Capability-gated network stack access
```

### GPU Driver [MUCH LATER]

```text
Mining Target: Linux DRM/KMS + Mesa
Dialect Surface:
  - DRM ioctl interface
  - GPU command submission
  - Memory management (GEM, TTM)
  - Display controller

Native Phorensic GPU Driver:
  - GPU MMIO access (capability-gated)
  - GPU command submission (capability-gated, bounded)
  - GPU memory management (fixed pools, no heap)
  - Display controller access (capability-gated)
  - Court-verified command submission (CPU reference path first)
```

## Driver Trust Progression Example

```text
// Serial driver loaded
let serial = DriverManager::load("serial-uart.phor-spec")?
// trust_state: Unknown
// capabilities: mmio(UART_BASE..UART_BASE+0x20), irq(4)

// Driver initializes
serial.init()?
let residuals = serial.extract_residuals()
// residuals show: device detected, baud set, TX ready
// Trust promotes: Unknown → Observed

// Driver transmits
serial.write_byte(0x41)?  // 'A'
let residuals = serial.extract_residuals()
// residuals show: byte 0x41 written to TX register, TX empty IRQ received
// Behavior matches oracle (reference UART specification)
// Trust promotes: Observed → Replayed

// After 1000 successful transmissions with identical residual pattern
// Court session reviews
let court = CourtSession::open(serial, Oracle::hardware("uart-16550"))
court.submit_evidence(serial.get_evidence_log())?
let replay = court.run_replay()?
let comparison = court.compare_with_oracle()?
// Verdict: Accept
// Trust promotes: Replayed → OracleCompared → ResidualStable → Sealed → Promoted

// Now driver may request additional capabilities
// (e.g., DMA for faster transmission)
```

## Driver Isolation

Drivers run in capability-isolated contexts:

- No ambient access to kernel memory
- No ambient access to other drivers' memory
- No ambient access to user data
- All hardware access through capability gates
- All inter-driver communication through typed ports

## Example: Driver Residual Record

```text
ResidualRecord:
  driver_id: "serial-uart-01"
  operation: "write_byte"
  input: { byte: 0x41 }
  output: { status: TXComplete }
  mmio_access: [{ address: 0x3F8, value: 0x41, type: write }]
  irq_handled: { irq: 4, count: 1 }
  timestamp: 12345
  hash: 0xABCD...
```
