# RuleBundle 分发容量画像

本画像是 #111 的本地可复核 smoke profile，不是生产容量承诺。生产部署仍需按真实网络、
租户、Bundle 大小和节点拓扑重新测量。

## 冻结输入

| 维度 | 冻结值 |
| --- | --- |
| HTTP clients | 1（单线程 p99 smoke）+ 16 并发读取负载 |
| tenants | 1（租户隔离接口保留在部署边界） |
| Bundle 大小 | 23 B smoke；实现上限 128 MiB |
| 缓存比例 | cold miss、warm hit、restart hit、offline hit 各有可执行 fixture |
| Server 通用工作线程 | core 4 / max 32 |
| Server 通用排队任务 | 256（有界） |
| Server 交付读取线程 | core 2 / max 16 |
| Server 交付读取排队任务 | 128（有界；满载返回 DELIVERY_OVERLOADED） |
| Server catalog | 256 个对象 / 512 MiB |
| Runner cache | 256 个对象 / 512 MiB |
| Runner response 上限 | 180005548 B（含 base64/JSON framing） |
| 目标错误语义 | 过载、超时、摘要/身份/信任失败均不开始扫描 |

## 最近一次本地结果

`RuleOpsHttpServerTest.deliveryCapacityProfileCapturesLatencyPercentilesWithBoundedReadPool`
在 128 次本地 loopback 请求上输出 p50/p95/p99；每次验收脚本重新采集并写入
`.scratch/bundle-delivery-e2e-check/bundle-delivery-e2e.md`，不把机器相关的毫秒数伪装成
稳定基线。

## 资源与隐私边界

- Server 交付读路径不访问治理 journal；journal/audit 的顺序持久性由既有 Server persistence
  seam 负责。
- Runner 对响应、decoded Bundle、cache 对象、materialized Bundle 和并发 single-flight 均设
  硬上限；缓存对象只创建、不覆盖，并在接收后进入摘要验证。
- 指标和证据只记录 digest、descriptor identity、延迟、队列/错误类别和字节数，不记录源码、
  规则正文或完整 Finding。Server catalog 暴露只读的计数快照，覆盖读取、命中/未命中、响应、
  字节、过载和最大队列深度；测试断言这些计数与请求数一致。
- 缓存命中、交付成功和单飞合并不授予 Gate、SCM 或治理审批权。
