# oscquery_debug

SlimeVR/VRChat OSCQuery debugger. Discovers OSCQuery services via mDNS and probes their HOST_INFO and OSC address tree.

## Build

```bash
cargo build --release
```

## Usage

**Browse** for OSCQuery services on the local network (default 15 seconds):

```bash
oscquery_debug browse
oscquery_debug browse --seconds 30
oscquery_debug browse --instance-filter vrchat
```

**Query** a specific endpoint (host:port or full URL):

```bash
oscquery_debug query 192.168.1.100:9000
oscquery_debug query http://localhost:9000
```

**Auto** — browse, then query the first matching service:

```bash
oscquery_debug auto slimevr
oscquery_debug auto vrchat --seconds 10
```
