# ERF Contract Conformance Suite

`erf-conformance` is the versioned, backend-neutral runner for the Backend
Host Contract v0. It starts a fresh host process for every protocol check and
also runs an independent local Gate acceptance set over typed contract values.
The local checks cover canonical Finding ordering/deduplication, message
independence, orthogonal Baseline/Exception decisions, malformed observations,
and the rule that incomplete execution remains `INCONCLUSIVE`.

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

The reproducible Phase 2 acceptance evidence is generated with:

```text
scripts/phase2-independent-acceptance.sh
```

The reference fake host exposes deterministic failure-injection controls for
the suite: `FAKE_POLLUTE_STDOUT`, `FAKE_SUPPORTED_MAJOR`,
`FAKE_BAD_CORRELATION`, `FAKE_DELAY_MS`, and `FAKE_CRASH_ON_EXECUTE`.
Protocol bytes belong on stdout; diagnostics belong on stderr.
