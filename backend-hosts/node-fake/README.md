# Node.js fake Host

`host.js` and `resolver.js` are dependency-free implementations of the Backend Host and Workspace Resolution contracts v0
for ADR-0086 language-neutrality tests. It shares the repository protobuf field
numbers and framing contract, emits the same canned observations as the Rust
reference fake, and intentionally contains no JavaScript analysis engine.

Run it through the conformance suite with:

```text
cargo run -p erf-conformance -- --host node \
  --host-arg backend-hosts/node-fake/host.js \
  --resolver node --resolver-arg backend-hosts/node-fake/resolver.js \
  --cancel-window-ms 150
```

The resolver is dependency-free and SAFE_STATIC only: it reads Java source
files and emits descriptors with SHA-256 file bindings; it never executes a
project build script.

For ExecuteComplete negative conformance, the host mirrors the Rust fake's
test-only controls: `FAKE_BAD_UNITS_REPORTED`,
`FAKE_BAD_OBSERVATION_COUNT`, `FAKE_BAD_ATTEMPT_DIGEST`, and
`FAKE_STATUS_ID_MODE=duplicate|unknown|blank`. These controls alter only
existing protocol fields and are exercised by
`scripts/execute-complete-negative-check.sh`.
