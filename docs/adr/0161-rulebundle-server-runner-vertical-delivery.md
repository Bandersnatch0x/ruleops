# ADR-0161: RuleBundle Server→Runner 垂直分发闭环

- Status: Accepted
- Date: 2026-09-20
- Scope: issue #111

## 决策

Server 通过 `GET /v1/rule-bundles/{bundleDigest}/delivery` 提供封闭的、只读的
`rule-bundle-delivery-v1` 响应。响应携带 descriptor、签名/信任/撤回/Pack/上下文检查
结果和有界的 `bundleBytesBase64`；Server 的交付读路径使用独立的不可变 catalog，不读取
治理 journal，也不持有源码、Finding 全文或 SCM 写入凭证。

Runner 使用显式 `http://host:port/base` 端点和固定的响应上限。Connected profile 只在
缓存 miss 时发起获取；接收后检查 HTTP 长度、JSON 闭合字段、descriptor 身份、Bundle
摘要和所有 verification facts，再以内容寻址、原子只创建 cache 保存。Enterprise
Air-gapped/Standalone profile 不因 Server 不可用而降级为未知资产。

同一 digest 的并发冷 miss 由 `SingleFlightDelivery` 合并；失败的 flight 可重试，成功的
结果由不可变 cache 稳定复用。Runner 在启动 resolver/backend 前完成交付解析，并要求
Server 返回的 canonical bundle bytes 与本地 Candidate 的 canonical payload 完全一致；若调用方
未提供本地 Candidate，Runner 会在同一内容寻址缓存根下安全 materialize 该有界 payload，再做
同一份 Candidate 校验。然后把 bundle digest、descriptor digest、compatibility 和 platform
注入实际 host 环境，同时写入 `ScanResult`/scan-output identity。

Server HTTP executor 使用固定核心/最大线程数和有界队列；交付读取另有独立的固定线程池和
有界队列，满载明确返回 `DELIVERY_OVERLOADED`，且交付 catalog 使用并发 map，故高频读取不
经过 `RuleOpsStore` 的治理写锁。这个本地垂直切片冻结了 128 MiB Bundle、180005548 字节
HTTP/JSON 响应上限、Runner cache 256 个对象/512 MiB、Server catalog 256 个对象/512 MiB、
交付读取 2/16 个线程和 128 个排队任务；租户级配额、TLS、对象存储、KMS/HSM 与生产拓扑
仍属于部署边界。

## 验证

```text
cargo test -p erf-bundle-delivery --all-targets
mvn -pl control-plane/server -am -Dtest=RuleOpsHttpServerTest test
bash scripts/bundle-delivery-e2e-check.sh .scratch/bundle-delivery-e2e-check
bash scripts/check.sh
```

验收覆盖 cold fetch、warm/restart hit、offline hit/miss、摘要/descriptor/签名/过期失败、
同 digest single-flight、并发 HTTP 读取与 p50/p95/p99 smoke profile。可用性、缓存命中和
交付身份不等同于 Gate authority。
