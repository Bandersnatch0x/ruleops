# ERF Contract Conformance Suite

`erf-conformance` 是面向 Backend Host Contract v0 与 Workspace Resolution
Contract v0 的版本化、Backend-neutral runner。需要进程隔离的协议 probe 会为
每个 Host/Resolver 会话启动新进程；golden、wire 兼容性和 local Gate 检查
则直接消费 typed contract value。Local 检查覆盖 Finding 的规范排序与去重、消息独立性、Baseline 与
Exception 的正交决策、畸形 observation，以及不完整执行必须保持
`INCONCLUSIVE` 的约束。

`golden/` 保存 Runtime→Host、Host→Runtime 以及 Runtime→Resolver、
Resolver→Runtime 的 canonical Protobuf wire fixture。套件每次运行都会解码并
逐字节重新编码这些 fixture，验证 wire compatibility。

构建 workspace 后运行 reference Host 与 Resolver：

```text
cargo run -p erf-conformance -- \
  --host target/debug/erf-fake-host \
  --resolver target/debug/erf-fake-resolver
```

使用受支持的延迟窗口启用 cooperative-cancel probe：

```text
cargo run -p erf-conformance -- \
  --host target/debug/erf-fake-host \
  --resolver target/debug/erf-fake-resolver \
  --cancel-window-ms 150
```

同一套 33 项检查也直接驱动无依赖的 Node.js Fake Host 与 Workspace
Resolver；这条路径用于证明协议实现不依赖 Rust 语言。Node fake 只实现
协议探针，不承担 JavaScript 规则分析：

```text
node --version
cargo run -p erf-conformance -- \
  --host node --host-arg backend-hosts/node-fake/host.js \
  --resolver node --resolver-arg backend-hosts/node-fake/resolver.js \
  --cancel-window-ms 150
```

使用以下命令生成可复现的 Phase 2 验收证据：

```text
scripts/phase2-independent-acceptance.sh
```

ExecuteComplete 的跨语言负例（Rust Fake 与 Node Fake）可单独复跑：

```text
scripts/execute-complete-negative-check.sh
```

该脚本注入重复、未知和空白 `UnitStatus.unit_id`，并确认完整性检查拒绝
每一种状态集合；同一轮正向验收还会注入不一致的
`units_reported`、`observation_count` 和 `attempt_digest`。这些都是既有
协议字段的测试夹具，不改变 Java Host、生产 trust 或协议 schema。

Reference Fake Host 与 Resolver 提供确定性的 failure-injection 控制。Host
控制项包括 `FAKE_POLLUTE_STDOUT`、`FAKE_SUPPORTED_MAJOR`、
`FAKE_BAD_CORRELATION`、`FAKE_DELAY_MS`、`FAKE_CRASH_ON_EXECUTE`、
`FAKE_BAD_UNITS_REPORTED`、`FAKE_BAD_OBSERVATION_COUNT`、
`FAKE_BAD_ATTEMPT_DIGEST` 和负例专用的
`FAKE_STATUS_ID_MODE=duplicate|unknown|blank`；Resolver
控制项包括 `FAKE_SUPPORTED_MAJOR`、`FAKE_PROTOCOL_MINOR`、
`FAKE_RESOLVE_DELAY_MS`、`FAKE_CRASH_ON_RESOLVE`、
`FAKE_MALFORMED_ON_RESOLVE` 与 `FAKE_OVERSIZED_ON_RESOLVE`。协议字节只能写入
stdout，诊断信息写入 stderr。

Node fake 支持同名的协议故障注入项（并额外支持
`FAKE_MAX_FRAME_BYTES`、`FAKE_PROTOCOL_MINOR`、`FAKE_UNKNOWN_OPTIONAL_ON_HELLO`
和 SAFE_STATIC symbolic-link 拒绝），因此不会因为换语言而缩小验收范围。
