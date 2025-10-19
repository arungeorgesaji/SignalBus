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

## Commands 

### Start Daemon 

Run the SignalBus daemon (P.S: Needed for SignalBus to function. Run this in a seperate terminal window):

```bash
signalbus daemon 
```

### Emit Signals 

Send signals with optional payload and TTL (Time-to-Live)

```bash
signalbus emit <SIGNAL_NAME> [--payload <JSON>] [--ttl <SECONDS>]
```

### Examples

```bash
signalbus emit user.created --payload '{"id": 123, "name": "john"}'
signalbus emit build.completed --ttl 300
signalbus emit system.alert --payload '{"level": "high"}' --ttl 60
```

### Listen to Signals 

Subscribe to signal patterns and optionally execute commands:

```bash
signalbus listen <PATTERN> [--exec <COMMAND>] 
```

### Examples

```bash
signalbus listen user.*
signalbus listen build.* --exec "./deploy.sh"
signalbus listen system.* --exec "notify-send 'System Event'"
``` 

### View Signal History 

Show recent signals matching a pattern: 

```bash
signalbus history <PATTERN> [--limit <NUMBER>]
```

### Examples

```bash 
signalbus history user.*
signalbus history test:* --limit 20
signalbus history system.alert --limit 5
```

### Rate Limiting 

Configure rate limits for signal patterns: 

```bash
signalbus rate-limit <PATTERN> <MAX_SIGNALS> --per-seconds <SECONDS> 
```

### Examples

```bash
signalbus rate-limit user.login 5 --per-seconds 60
signalbus rate-limit system.alert 10 --per-seconds 30
signalbus rate-limit test:* 3 --per-seconds 10
```

### View Rate Limits 

Show currently configured rate limits: 

```bash
signalbus show-rate-limits 
```

## Use Cases  

- **Build Systems** - build.started -> build.completed -> deploy.triggered
- **User Management** - user.created -> email.welcome -> analytics.track
- **System Monitoring** - system.cpu.high -> alert.notify -> scale.up
- **Development Workflows** - test.failed -> slack.notify -> retry.build

## Pattern Matching

Use wildcards to subscribe to multiple signals:

* `user.*` - matches `user.created`, `user.updated`, `user.deleted`
* `*.completed` - matches `build.completed`, `test.completed`, `deploy.completed`
* `system.*.high` - matches `system.cpu.high`, `system.memory.high`

## Environment Variables (for --exec)

When using `--exec`, your command receives these environment variables:

* `SIGNALBUS_SIGNAL` - The signal name that was emitted
* `SIGNALBUS_PAYLOAD` - The signal payload as JSON string (or "null" if no payload)
* `SIGNALBUS_TIMESTAMP` - When the signal was emitted

**Example script:**
```bash
#!/bin/bash
echo "Received signal: $SIGNALBUS_SIGNAL"
echo "Payload: $SIGNALBUS_PAYLOAD"
echo "Timestamp: $SIGNALBUS_TIMESTAMP"
```
