# ERF Contract Conformance Suite

`erf-conformance` is the versioned, backend-neutral runner for the Backend
Host Contract v0. It starts a fresh host process for every check and prints a
per-check report containing the suite version.

The `golden/` directory contains canonical protobuf wire fixtures for the
runtime-to-host `Hello` and host-to-runtime `HelloAck` directions. The suite
decodes and re-encodes both fixtures byte-for-byte on every run.

Build the workspace, then run the reference host:

```text
cargo run -p erf-conformance -- --host target/debug/erf-fake-host
```

Enable the cooperative-cancel probe with a host-supported delay window:

```text
cargo run -p erf-conformance -- --host target/debug/erf-fake-host \
  --cancel-window-ms 150
```

The reference fake host exposes deterministic failure-injection controls for
the suite: `FAKE_POLLUTE_STDOUT`, `FAKE_SUPPORTED_MAJOR`,
`FAKE_BAD_CORRELATION`, `FAKE_DELAY_MS`, and `FAKE_CRASH_ON_EXECUTE`.
Protocol bytes belong on stdout; diagnostics belong on stderr.
