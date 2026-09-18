# Node.js fake Host

`host.js` and `resolver.js` are dependency-free implementations of the Backend Host and Workspace Resolution contracts v0
for ADR-0086 language-neutrality tests. It shares the repository protobuf field
numbers and framing contract, emits the same canned observations as the Rust
reference fake, and intentionally contains no JavaScript analysis engine.

Run it through the conformance suite with:

```text
cargo run -p erf-conformance -- --host node \
  --host-arg backend-hosts/node-fake/host.js \
  --cancel-window-ms 150
```

Run `node backend-hosts/node-fake/resolver.js` with the Rust resolver client to
exercise `ecosystem.static-layout`; it emits SAFE_STATIC descriptors with
SHA-256 file bindings.
