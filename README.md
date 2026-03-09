# upnpsim

A UPnP IGD (Internet Gateway Device) router simulator for testing. It
implements the WANIPConnection:1 service so that UPnP client code--whether
using `upnpc`, the `igd` crate, or raw SOAP--can be exercised against a
realistic, fully controllable target without real hardware.

## Features

- **Full WANIPConnection:1 service** -- AddPortMapping, DeletePortMapping,
  GetExternalIPAddress, GetSpecificPortMappingEntry,
  GetGenericPortMappingEntry, GetStatusInfo
- **SSDP discovery** -- responds to M-SEARCH queries and sends periodic
  ssdp:alive notifications so clients can discover the simulated gateway
  automatically
- **GENA eventing** -- SUBSCRIBE / UNSUBSCRIBE with SID tracking, renewal,
  and timeout-based expiry
- **Virtual clock with time-shift** -- advance the daemon's clock by
  arbitrary durations to test TTL expiry without waiting
- **TTL modes** -- `respect` (default) expires mappings and subscriptions on
  schedule; `disrespect` keeps everything alive indefinitely
- **Runtime control socket** -- change the external IP, list mappings, shift
  time, toggle TTL mode, query status, and shut down gracefully--all via a
  Unix domain socket with a simple JSON protocol
- **UPnP-compliant XML** -- device description, SCPD, SOAP envelopes, and
  UPnP error faults all follow the spec

## Installation

### From source

```
cargo install --path .
```

### Build only

```
cargo build --release
```

The binary is at `target/release/upnpsim`.

## Quick start

Start the daemon:

```
upnpsim start
```

This listens on `0.0.0.0:5000` for HTTP/SOAP/GENA, binds the SSDP multicast
group on all interfaces, and creates a control socket at `/tmp/upnpsim.sock`.

In another terminal, use any UPnP client:

```
# Using miniupnpc
upnpc -u http://127.0.0.1:5000/device.xml -s
upnpc -u http://127.0.0.1:5000/device.xml -a 192.168.1.50 80 8080 TCP 3600
upnpc -u http://127.0.0.1:5000/device.xml -l

# Or use the control CLI
upnpsim status
upnpsim list-mappings
```

## CLI reference

### `upnpsim start`

Launch the simulator daemon.

| Flag | Default | Description |
|------|---------|-------------|
| `--listen` | `0.0.0.0:5000` | HTTP listen address |
| `--external-ip` | `203.0.113.1` | WAN IP reported to clients |
| `--interface` | `0.0.0.0` | Network interface for SSDP multicast |
| `--ttl-mode` | `respect` | `respect` or `disrespect` lease TTLs |
| `--socket-path` | `/tmp/upnpsim.sock` | Unix socket for control commands |

### `upnpsim status`

Print daemon status: uptime, clock offset, TTL mode, external IP, mapping
count, subscription count, device UUID.

### `upnpsim list-mappings`

List all active port mappings with protocol, ports, internal client, enabled
state, description, lease duration, and remaining lease seconds.

### `upnpsim set-external-ip <IP>`

Change the external IP the daemon reports at runtime.

```
upnpsim set-external-ip 198.51.100.42
```

### `upnpsim set-ttl-mode <MODE>`

Switch between `respect` (honor lease durations) and `disrespect` (keep
everything alive forever).

```
upnpsim set-ttl-mode disrespect
```

### `upnpsim time-shift <DURATION>`

Advance the virtual clock by the given duration. Accepts human-readable
formats like `30s`, `5m`, `2h30m`.

```
upnpsim time-shift 2h
```

Combined with TTL mode `respect`, this lets you test lease expiry
deterministically without waiting.

### `upnpsim shutdown`

Gracefully stop the daemon.

All subcommands that talk to a running daemon accept `--socket-path` to
specify a non-default control socket location.

## HTTP endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/device.xml` | UPnP root device description |
| GET | `/scpd/WANIPConn1.xml` | WANIPConnection:1 service description |
| POST | `/ctl/WANIPConn1` | SOAP action endpoint |
| SUBSCRIBE | `/evt/WANIPConn1` | GENA subscription |
| UNSUBSCRIBE | `/evt/WANIPConn1` | GENA unsubscription |

## Testing

```
make test
```

This runs clippy (warnings are fatal) followed by `cargo test`, which
executes:

- **23 integration tests** covering SOAP actions, GENA subscriptions, control
  socket commands, HTTP endpoints, TTL expiry with time-shift, and error cases
- **7 upnpc tests** that exercise the daemon through the real `upnpc` binary
  (from miniupnpc), cross-verifying results against the SOAP API. These skip
  gracefully if `upnpc` is not installed.

## Architecture

```
                 +-----------+
  SSDP M-SEARCH |           |  HTTP GET /device.xml, /scpd/...
  ──────────────>   upnpsim  <──────────────────────────────────
                 |           |  POST /ctl/WANIPConn1 (SOAP)
  SSDP NOTIFY   |  daemon   |  SUBSCRIBE/UNSUBSCRIBE /evt/...
  <──────────────|           |──────────────────────────────────>
                 +-----------+
                      |
              Unix socket (JSON)
                      |
                 +-----------+
                 |  upnpsim  |
                 |   CLI     |
                 +-----------+
```

The daemon spawns four concurrent tasks:

1. **HTTP server** (hyper) -- serves device descriptions, dispatches SOAP
   actions, handles GENA subscriptions
2. **SSDP listener** -- responds to M-SEARCH discovery and sends periodic
   alive notifications
3. **TTL reaper** -- ticks every second, removes expired mappings and
   subscriptions (when in `respect` mode)
4. **Control server** -- accepts commands over a Unix domain socket

All tasks share state through `Arc<RwLock<SimState>>` and coordinate shutdown
via a `tokio::sync::watch` channel.

## License

Licensed under either of

- GNU Affero General Public License, Version 3.0
  ([LICENSE](LICENSE) or https://www.gnu.org/licenses/agpl-3.0.html)
