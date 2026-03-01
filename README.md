# oscquery_debug

SlimeVR/VRChat OSCQuery debugger. Discovers OSCQuery services via mDNS, probes their HOST_INFO and OSC address tree, and can stub a SlimeVR OSCQuery listener to receive live tracking data.

## Build

```bash
cargo build --release
```

## Usage

### Browse

Discover OSCQuery services on the local network. Results are printed live as they're found.

```bash
oscquery_debug browse
oscquery_debug browse 30
```

### Query

Probe a specific OSCQuery endpoint for HOST_INFO and the OSC address tree.

```bash
oscquery_debug query 192.168.1.100:9000
oscquery_debug query http://localhost:9000
```

### Listen

Stub SlimeVR's OSCQuery role: advertise mDNS services, serve OSCQuery HTTP, and listen for incoming OSC tracking data with a live TUI display.

```bash
oscquery_debug listen
oscquery_debug listen --osc-port 9002
oscquery_debug listen --interface 192.168.1.50
```
