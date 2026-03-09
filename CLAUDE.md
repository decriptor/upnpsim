# CLAUDE.md

## Project overview

upnpsim is a UPnP IGD (Internet Gateway Device) router simulator. It
implements the WANIPConnection:1 SOAP service, SSDP discovery, and GENA
eventing so UPnP clients can be tested without real hardware.

## Build and test

```
make build    # cargo build
make test     # cargo check && cargo clippy -- -D warnings && cargo test
make lint     # cargo check && cargo clippy -- -D warnings
make install  # cargo install --path .
```

`make test` runs clippy with `-D warnings` (warnings are fatal) before tests.

## Architecture

- `src/daemon.rs` -- orchestrator: spawns HTTP, SSDP, reaper tasks + control server
- `src/http_server.rs` -- hyper HTTP server, routes to SOAP/GENA/description handlers
- `src/soap.rs` -- SOAP XML parsing (quick-xml) and WANIPConnection action dispatch
- `src/eventing.rs` -- GENA SUBSCRIBE/UNSUBSCRIBE with SID tracking
- `src/ssdp.rs` -- SSDP multicast listener and M-SEARCH/NOTIFY responder
- `src/control.rs` -- Unix domain socket control server (JSON protocol)
- `src/protocol.rs` -- Request/Response types for the control socket
- `src/state.rs` -- shared state: mappings, subscriptions, TTL mode, external IP
- `src/clock.rs` -- lock-free virtual clock with time-shift support
- `src/description.rs` -- UPnP device.xml and SCPD XML generation
- `src/cli.rs` -- clap CLI definitions

State is shared via `Arc<RwLock<SimState>>`. Shutdown coordination uses
`tokio::sync::watch`. The virtual clock uses `Arc<AtomicI64>` for lock-free
reads.

## Test conventions

- Integration tests in `tests/integration.rs` start an in-process daemon per
  test using `TestDaemon::start()` which binds a random port and unique socket
  path. Cleanup is in `Drop`.
- `tests/upnpc.rs` tests require the `upnpc` binary (miniupnpc) and skip
  gracefully if not found. The `upnpc()` helper uses `spawn_blocking` to avoid
  blocking the tokio runtime.
- SOAP helpers (`soap_request`, `add_mapping_raw`, `extract_xml_value`) are
  duplicated in each test file since they're test utilities.

## Key behaviors

- Port mapping uniqueness is per `(protocol, external_port)` pair
- `lease_duration == 0` means permanent (never expires)
- GENA renewal does NOT update the stored expiry
- Unknown SOAP action returns HTTP 401; UPnP errors return HTTP 500
- TTL reaper ticks every 1 second in `respect` mode, skips in `disrespect`
