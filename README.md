# SignalBus

SignalBus is a **lightweight, local-first developer framework** for structured signals and events. It enables processes, scripts, and applications on the same machine to communicate instantly through high-level signals **without the overhead of HTTP APIs or message brokers**.

---

## Features

- **Near-instant event dispatching** for local processes.  
- **Simple CLI and library interface** for emitting, listening, and reacting to signals.  
- **Multi-language integration** for rapid automation, tool chaining, and developer workflows.  
- **Authentication & Authorization** - Token-based security with fine-grained permissions 
- **Token Management** - Create, revoke, and manage access tokens for different users and applications

---

## Use Cases

- Automate local build/test/deploy pipelines. 
- Connect TUI or CLI tools with real-time event triggers.
- Monitor and react to system or application signals.
- Prototype event-driven developer frameworks quickly.
- Secure multi-user environments where different users/processes need isolated signal access 
- Service-to-service communication with controlled permissions

---

## Key Features

1. **Emit Signals:** Send structured events with optional payloads.  
2. **Listen Signals:** Subscribe to patterns or topics and react automatically.  
3. **Daemon-Based:** Central hub that routes signals efficiently across processes.  
4. **Lightweight & Minimal Setup:** Works on a single machine without servers or brokers.  
6. **Authentication:** Token-based access control for all operations.
7. **Permission System:** Fine-grained permissions (Read, Write, History, RateLimit, Admin) 
8. **Token Management:** Create and revoke tokens with specific permissions and expiration

---

## Advanced Features 

- **Signal Persistence** - Store emitted signals temporarily so late-joining listeners can replay or catch up on missed events.
- **Signal History** - Query recent signals from the daemon, including timestamp, sender, and payload metadata. 
- **Rate Limiting** - Prevent signal spam by defining per-topic or per-sender emission limits. 
- **Authentication** - Use token-based authentication to isolate users or processes in multi-user systems.
- **Signal TTL (Time-to-Live)** - Automatically expire old signals to keep memory and event queues clean.
- **Pattern Matching** - Subscribe using wildcards to handle dynamic or hierarchical topics (build:*, system.cpu:*).
- **Priority Signals** - Optional priority queueing — urgent events can bypass normal rate limits.
- **Scoped Listeners** - Limit listeners to specific namespaces or users for better isolation and debugging. 

## Install 

Install via Cargo:

```bash
cargo install signalbus
```

## Commands 

### Login

Authenticate and save your token for future commands:

```bash
signalbus login --user-id <USER_ID> --password <PASSWORD> 
```

### Logout

Remove your saved authentication token: 

```bash
signalbus logout 
```

### Create Token

Generate a new token with specific permissions (requires Admin permission):

```bash
signalbus create-token --user-id <USER_ID> --permissions Read,Write [--expires-in <SECONDS>]
``` 

### Revoke Token 

Invalidate a token (requires Admin permission): 

```bash
signalbus revoke-token <TOKEN> [--admin-token <ADMIN_TOKEN>] 
```

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
signalbus emit user:created --payload '{"id": 123, "name": "john"}'
signalbus emit build:completed --ttl 300
signalbus emit system:alert --payload '{"level": "high"}' --ttl 60
```

### Listen to Signals 

Subscribe to signal patterns and optionally execute commands:

```bash
signalbus listen <PATTERN> [--exec <COMMAND>] 
```

### Examples

```bash
signalbus listen user:*
signalbus listen build:* --exec "./deploy.sh"
signalbus listen system:* --exec "notify-send 'System Event'"
``` 

### View Signal History 

Show recent signals matching a pattern: 

```bash
signalbus history <PATTERN> [--limit <NUMBER>]
```

### Examples

```bash 
signalbus history user:*
signalbus history test:* --limit 20
signalbus history system:alert --limit 5
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

## Permissions 

SignalBus comes with the default admin user `admin` with password `admin123`.

- **Read** - Listen to signals and view history 
- **Write** - Emit signals 
- **History** - Access signal history 
- **RateLimit** - Configure rate limits for signals 
- **Admin** - Create/revoke tokens and manage permissions

## Token Storage  

Tokens are automatically saved to ~/.signalbus_token after login. Most commands will use this token unless you specify --token.

## Pattern Matching

Use wildcards to subscribe to multiple signals:

* `user:*` - matches `user.created`, `user.updated`, `user.deleted`
* `*.completed` - matches `build.completed`, `test.completed`, `deploy.completed`
* `system:*.high` - matches `system.cpu.high`, `system.memory.high`

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

---
> **Note:** SignalBus currently keeps all signal data in memory and clears it when the daemon stops.
Optional persistence across restarts will be added later, allowing data and configurations to be retained between daemon sessions if desired.  
---
