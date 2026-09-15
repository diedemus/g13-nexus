# Contributing

Contributions are welcome. Keep hardware behavior grounded in verified Linux input events and documented/captured G13 behavior; do not invent HID report formats.

Before submitting a change:

```bash
cargo fmt --check
cargo check --all-targets
cargo test
```

Preserve the established hardware layout unless a UI change is intentional and documented. Input latency is a product requirement: ordinary G13 controls and mode keys should not perform disk, LCD, or other slow I/O on the event-critical path.
