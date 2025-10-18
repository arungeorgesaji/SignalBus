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

- Automate local build/test/deploy pipelines. 
- Connect TUI or CLI tools with real-time event triggers.
- Monitor and react to system or application signals.
- Prototype event-driven developer frameworks quickly.

---

## Key Features

1. **Emit Signals:** Send structured events with optional payloads.  
2. **Listen Signals:** Subscribe to patterns or topics and react automatically.  
3. **Daemon-Based:** Central hub that routes signals efficiently across processes.  
4. **Lightweight & Minimal Setup:** Works on a single machine without servers or brokers.  
5. **OS Integration:** Optionally map native signals to high-level events.  

---

## Advanced Features 

- **Signal Persistence** - Store emitted signals temporarily so late-joining listeners can replay or catch up on missed events.
- **Signal History** - Query recent signals from the daemon, including timestamp, sender, and payload metadata. 
- **Rate Limiting** - Prevent signal spam by defining per-topic or per-sender emission limits. 
- **Authentication** - Use token-based authentication to isolate users or processes in multi-user systems.
- **Signal TTL (Time-to-Live)** - Automatically expire old signals to keep memory and event queues clean.
- **Pattern Matching** - Subscribe using wildcards (*, ?, etc.) to handle dynamic or hierarchical topics (build:*, system.cpu.*).
- **Priority Signals** - Optional priority queueing — urgent events can bypass normal rate limits.
- **Scoped Listeners** - Limit listeners to specific namespaces or users for better isolation and debugging. 

## Example

```bash
# Emit a build completion signal
signalbus emit build:done --payload "{time: 123, success: true}"

# Listen for all build-related signals and run a command
signalbus listen build:* --exec "./scripts/deploy.sh"
