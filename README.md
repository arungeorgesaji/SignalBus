# SignalBus

SignalBus is a **lightweight, local-first developer framework** for structured signals and events. It enables processes, scripts, and applications on the same machine to communicate instantly through high-level signals **without the overhead of HTTP APIs or message brokers**.

---

## Features

- **Unifies OS-level signals** (e.g., `SIGUSR1`) with user-defined events.  
- **Near-instant event dispatching** for local processes.  
- **Simple CLI and library interface** for emitting, listening, and reacting to signals.  
- **Multi-language integration** for rapid automation, tool chaining, and developer workflows.  

---

## Use Cases

- Automating build/test/deploy pipelines locally.  
- Connecting TUI or CLI tools with real-time event triggers.  
- Monitoring and reacting to system or application signals.  
- Rapid prototyping of event-driven developer frameworks.  

---

## Key Features

1. **Emit Signals:** Send structured events with optional payloads.  
2. **Listen Signals:** Subscribe to patterns or topics and react automatically.  
3. **Daemon-Based:** Central hub that routes signals efficiently across processes.  
4. **Lightweight & Minimal Setup:** Works on a single machine without servers or brokers.  
5. **OS Integration:** Optionally map native signals to high-level events.  

---

## Example

```bash
# Emit a build completion signal
signalbus emit build:done --payload "{time: 123, success: true}"

# Listen for all build-related signals and run a command
signalbus listen build:* --exec "./scripts/deploy.sh"
