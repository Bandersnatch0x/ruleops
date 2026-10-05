# 域 7：安全与接口独立审计（重做）

## 域范围

标的：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md` §24–§29.1（L1301–1538，含 §25）。工作树：WSL `kali-linux2:/home/kali/文档/ruleops`；HEAD `7e7d2820de5f2948bd936b64f5907ae211f00645`。
本轮从指定规格章节与源码独立取证，未读取任何既有 `07-*.md`。只写此报告，不改源码、docs、scripts；没有运行网络命令、Cargo 或构建。
方法：规格章节/行号 → 最多 1–2 个关键证据位置 → 判定/理由。Threats 与 Controls 分别列项；“一致”仅覆盖该行声明，不代表安全认证。缺证据不等于证明功能不存在。
声明总数：**96**；一致 24 / 部分一致 40 / 不一致 10 / 未实现 15 / 无法验证 7；不一致 + 未实现：**25**（包含命令及布局漂移，不是同等严重的漏洞数）。

## 逐声明核验表：§24 Exceptions

| 声明（规格原文位置） | 代码实证 | 判定 | 理由 |
| --- | --- | --- | --- |
| **E01** §24 L1303：签名、不可变、可撤回 ExceptionGrant | runtime/governance-registry/src/lib.rs:235-256,356-370；runtime/gate/src/lib.rs:99-112 | 部分一致 | 有签名内容与按 digest 登记/撤回生命周期；端到端消费缺口见 E10。 |
| **E02** §24 L1327：exact scope | runtime/gate/src/lib.rs:130-146,165-180 | 部分一致 | 校验必填 repository/rule/scenario/path 与受限选择器；未在 Gate 消费路径看到 grant selector 匹配。 |
| **E03** §24 L1328：owner 必填 | runtime/gate/src/lib.rs:130-141 | 一致 | 统一非空及控制字符校验包含 owner。 |
| **E04** §24 L1329：reason 必填 | runtime/gate/src/lib.rs:130-141 | 一致 | 统一校验包含 reason。 |
| **E05** §24 L1330：expiry，禁止永久授权 | runtime/gate/src/lib.rs:151-153 | 一致 | 必须是有效有限时间区间，未生效/过期返回错误；仅指 grant 校验器。 |
| **E06** §24 L1331：approval 必填 | runtime/gate/src/lib.rs:148-149 | 一致 | 要求非空 approval 集合且每项非空。 |
| **E07** §24 L1332：content digest + exception-signing 角色签名 | runtime/governance-registry/src/lib.rs:469-476；runtime/gate/src/lib.rs:34,99-112 | 部分一致 | 入库验签使用角色与 canonical payload；不等于 Gate 使用时已验签。 |
| **E08** §24 L1333：撤回状态 | runtime/governance-registry/src/lib.rs:356-370 | 一致 | 注册表有显式 Revoked 生命周期；消费时检查另见 E10。 |
| **E09** §24 L1334：审计轨迹 | runtime/governance-registry/src/lib.rs:256,425-451 | 一致 | 生命周期写入 audit 向量并有序号；持久化/防篡改审计不在此证据范围。 |
| **E10** §24 L1336：Gate 只消费精确引用 digest 对应的有效 grant，失效/撤回/scope/签名不符视为不存在 | runtime/trusted-ci/src/lib.rs:165-173,229-233；runtime/gate/src/lib.rs:577-590 | 不一致 | trusted CI 直接将 request.exception_fingerprints 交给 Gate；Gate 仅按 fingerprint 集合豁免，grant digests 另行复制到待签 attestation。此边界没有从授权 digest 验证并派生豁免集，签署结果不弥补授权验证缺口；不宣称已证明外部攻击者能控制调用者。 |
| **E11** §24 L1338：不能豁免扫描未执行或其他 Completeness failure | runtime/gate/src/lib.rs:575-590 | 一致 | Completeness 不受例外集合影响，不完整优先 INCONCLUSIVE；身份可信度另由 trusted-context 检查。 |
| **E12** §24 L1340：封闭选择器：必填身份、根相对受限 glob、规范 symbol、精确 fingerprint、AND、禁止 regex/脚本/隐式全仓库 | runtime/gate/src/lib.rs:130-180；runtime/gate/src/lib.rs:577-580 | 部分一致 | 拒绝绝对 path、..、若干 regex 元字符和全仓库通配；symbol 仅拒绝空白/..。未找到 selector AND 匹配实现证据，Gate 只匹配 fingerprint。 |

## 逐声明核验表：§25 Feedback Loop

| 声明（规格原文位置） | 代码实证 | 判定 | 理由 |
| --- | --- | --- | --- |
| **F01** §25 L1346：Finding→Feedback→四类 Triage→Regression→Revision→Verification→Shadow→Release | runtime/feedback/src/lib.rs:32,96-102,188-205 | 部分一致 | 存在分类、台账与 regression-case handoff；本轮未验证新 Revision 到发布的完整编排。 |
| **F02** §25 L1370：真实 FP 默认沉淀 regression asset | runtime/feedback/src/lib.rs:96-102,203-205,275-281 | 部分一致 | 已找到根据分类自动生成 RegressionCase 的实现与测试断言；资产持久化与真实 FP 分类全链路未运行验证。 |

## §26.1 十三项 Threats × §26.2 Controls 对照表

| 声明（规格原文位置） | 代码实证 | 判定 | 理由 |
| --- | --- | --- | --- |
| **T01** §26.1 L1378：prompt injection → 对应 §26.2 控制：model 无 publish/signing 权限；Candidate worker sandbox | runtime/verification-worker/src/lib.rs:255-280；runtime/verification-worker/src/main.rs:45-69 | 部分一致 | worker 接收 signer 并先校验 sandbox proof；未找到模型权限/IAM 隔离实现证据。 |
| **T02** §26.1 L1379：malicious generated Java → 对应 §26.2 控制：Candidate worker sandbox；fixtures compile/analyze；resource quotas | runtime/sandbox/src/lib.rs:73-87；runtime/verification-worker/src/lib.rs:197-216 | 部分一致 | 有 rootless OCI/资源限制和签名隔离证明校验；不能据此认定所有 Candidate 编译执行链已接入。 |
| **T03** §26.1 L1380：malicious build script → 对应 §26.2 控制：Candidate worker sandbox；network deny by default | runtime/trusted-ci/src/lib.rs:119-138；runtime/sandbox/src/lib.rs:73-87 | 部分一致 | trusted CI 强制 Host/Resolver sandbox policy，OCI 禁网；具体 Maven/Gradle 恶意脚本路径未验证。 |
| **T04** §26.1 L1381：dependency poisoning → 对应 §26.2 控制：dependency allowlist / lock；SBOM；provenance | Cargo.lock；runtime/release/src/lib.rs:27-28,171-175 | 部分一致 | 有 Rust lockfile 与 release 的 SBOM/provenance digest 绑定；未找到实际依赖 allowlist 执行证据，锁版本也不等于可信来源。 |
| **T05** §26.1 L1382：artifact tampering → 对应 §26.2 控制：signed artifacts；provenance；immutable release | runtime/release/src/lib.rs:144-175,203-204；runtime/rf-cli/src/main.rs:812-849 | 部分一致 | 找到 release 输入 digest 检查和 CLI runtime manifest 签名信任根检查；未运行篡改负例。 |
| **T06** §26.1 L1383：forged scan result → 对应 §26.2 控制：trusted CI injects context；source identity validation；signed artifacts | runtime/trusted-ci/src/lib.rs:119-138,233-234；runtime/trusted-context/src/lib.rs:179-190,229-243 | 部分一致 | 签名/锁与 repository/commit/workload 身份绑定可见；外部 CI 的身份注入真实性未验证。 |
| **T07** §26.1 L1384：policy tampering → 对应 §26.2 控制：signed artifacts；trusted CI；project PR 不可修改 trusted gate | runtime/governance-registry/src/lib.rs:58-96；runtime/trusted-context/src/lib.rs:189-190 | 部分一致 | PolicyProfile 有验签接口并核对 policy digest；未找到 PR 不能修改 trusted gate 的平台执行证据，且例外输入闭合缺口见 E10。 |
| **T08** §26.1 L1385：baseline manipulation → 对应 §26.2 控制：signed artifacts；trusted CI injects context | runtime/trusted-context/src/lib.rs:230-235；runtime/gate/src/lib.rs:577-585 | 部分一致 | 核对 baseline digest，但 Gate 接受 Baseline 内容集合；本轮未验证集合与授权 digest 的派生链。 |
| **T09** §26.1 L1386：path traversal / symlink → 对应 §26.2 控制：Candidate worker sandbox；source identity validation（补充路径校验） | clients/vscode/src/path-safety.js:9-35；runtime/gate/src/lib.rs:165-174 | 部分一致 | VS Code 逐段拒绝 symlink 并做 realpath 包含检查，grant 拒绝 ..；未对全部读写路径给出覆盖结论。 |
| **T10** §26.1 L1387：zip bomb → 对应 §26.2 控制：resource quotas | runtime/sandbox/src/lib.rs:63,75；runtime/rf-runner/src/main.rs:2213-2232 | 部分一致 | 容器资源上限与材料化条目/文件大小上限有证据；未找到 ZIP 解压比、累计展开量/嵌套层级专门限制实现证据，也未确认 ZIP 输入是否受支持。 |
| **T11** §26.1 L1388：report injection → 对应 §26.2 控制：signed artifacts；source identity validation（不能替代输出编码） | 未找到实现证据；限界检索 runtime/verification-worker、runtime/release、backend-hosts、contracts，未完整追踪报告渲染器 | 无法验证 | §26.2 未显式列输出编码控制，不能以签名或路径安全推断 HTML/SARIF/终端注入已防护。 |
| **T12** §26.1 L1389：secret exfiltration → 对应 §26.2 控制：network deny by default；Candidate worker sandbox；model 无 signing 权限 | runtime/sandbox/src/lib.rs:75；runtime/verification-worker/src/main.rs:45-69 | 部分一致 | OCI 显式禁网、只传指定环境；候选工作进程凭据挂载与所有日志泄漏路径未验证。 |
| **T13** §26.1 L1390：cache poisoning → 对应 §26.2 控制：signed artifacts；revocation；immutable release | runtime/bundle-cache/src/lib.rs:206-229；runtime/rf-cli/src/registry_sync.rs:74 | 部分一致 | 读取缓存前检查 descriptor/有效期/verification evidence；证据真实性、缓存 bytes digest 及落盘并发细节未完整追踪。 |

## 逐声明核验表：§26.2 Controls

| 声明（规格原文位置） | 代码实证 | 判定 | 理由 |
| --- | --- | --- | --- |
| **C01** §26.2 L1394：Candidate worker sandbox | runtime/verification-worker/src/lib.rs:197-216,255-262；runtime/sandbox/src/lib.rs:73-87 | 部分一致 | 证明验签与沙箱构造存在；Candidate worker 的真实部署调用边界未完整验证。 |
| **C02** §26.2 L1395：model 无 publish/signing 权限 | runtime/verification-worker/src/main.rs:45-69 | 无法验证 | fixture signer 受 feature/env 门禁；未找到模型主体 IAM/publish 权限策略实现证据。 |
| **C03** §26.2 L1396：fixtures 默认 compile/analyze | 未找到实现证据；本轮未追踪 Java fixture 编译器和执行入口 | 无法验证 | 不可由测试名称推断 fixtures 没有被执行。 |
| **C04** §26.2 L1397：dependency allowlist / lock | Cargo.lock；runtime/release/src/lib.rs:171-175 | 部分一致 | lock 存在；未找到全链路 allowlist 执行证据。 |
| **C05** §26.2 L1398：SBOM | runtime/release/src/lib.rs:27,90,171-172 | 部分一致 | 仅确认 sbom_digest 的存在/比对；未验证 SBOM 生成、格式或内容覆盖。 |
| **C06** §26.2 L1399：signed artifacts | runtime/release/src/lib.rs:55-76,144-175；runtime/rf-cli/src/main.rs:812-849 | 部分一致 | 有签名/测量接口与 runtime manifest 信任根校验；不扩张为所有资产均受强制验签。 |
| **C07** §26.2 L1400：provenance | runtime/release/src/lib.rs:28,91,174-175 | 部分一致 | 有 provenance_digest 绑定；未验证 provenance 来源与内容真实性。 |
| **C08** §26.2 L1401：revocation | runtime/governance-registry/src/lib.rs:356-370；runtime/release/src/lib.rs:372-375 | 部分一致 | 有授权撤回状态与撤回密钥测试；使用点的授权撤回强制检查见 E10 缺口。 |
| **C09** §26.2 L1402：trusted CI injects context | runtime/trusted-ci/src/lib.rs:119-138；runtime/trusted-context/src/lib.rs:237-243 | 部分一致 | 代码验证注入的工作负载与锁身份；真实 CI 注入配置未验证。 |
| **C10** §26.2 L1403：project PR 不可修改 trusted gate | 未找到实现证据；本轮未核实托管平台 protected workflow / branch rules | 无法验证 | 这是部署/平台权限性质，不可仅从本地 Rust 代码确认。 |
| **C11** §26.2 L1404：source identity validation | runtime/trusted-context/src/lib.rs:184-190 | 部分一致 | 明确核对 repository/commit；输入是否由可信 SCM 实测注入未验证。 |
| **C12** §26.2 L1405：resource quotas | runtime/sandbox/src/lib.rs:63,75；runtime/verification-worker/src/lib.rs:197-207 | 一致 | OCI 配置内存/CPU/PID 非零上限，proof 要求相同类别控制；仅就沙箱控制实现判定。 |
| **C13** §26.2 L1406：network deny by default | runtime/sandbox/src/lib.rs:75；runtime/trusted-ci/src/lib.rs:120-122 | 一致 | provider 固定 --network=none，trusted CI 缺沙箱策略即失败；不代表部署实际通过运行测试。 |
| **C14** §26.2 L1407：immutable release | runtime/release/src/lib.rs:144-175；runtime/bundle-cache/src/lib.rs:342 | 部分一致 | 内容 digest 校验与不可变硬链接发布意图存在；release registry 覆盖写保护未完整核验。 |

## 逐声明核验表：§27 CLI

| 声明（规格原文位置） | 代码实证 | 判定 | 理由 |
| --- | --- | --- | --- |
| **L01** §27 L1414：rf standard validate | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L02** §27 L1415：rf coverage analyze | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L03** §27 L1417：rf rule init | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L04** §27 L1418：rf rule validate | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L05** §27 L1419：rf rule generate | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L06** §27 L1420：rf rule test | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L07** §27 L1421：rf rule submit | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L08** §27 L1423：rf release verify | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L09** §27 L1424：rf release promote | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L10** §27 L1425：rf release revoke | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L11** §27 L1427：rf scan | runtime/rf-cli/src/main.rs:989-1004 | 一致 | 有实际 scan 调度与参数解析。 |
| **L12** §27 L1428：rf gate | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L13** §27 L1429：rf explain | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L14** §27 L1431：rf feedback submit | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L15** §27 L1432：rf exception request | runtime/rf-cli/src/main.rs:989-1004 | 未实现 | main 仅接受 scan 与额外的 registry 分支；其他一级命令均退出 2。 |
| **L16** §27 L1439：scan 退出码 0/2/3/4，Finding 不改变完整扫描成功语义 | runtime/rf-cli/src/main.rs:55-63,1574-1584 | 一致 | 错误按 trust/input/incomplete 映射；完整扫描即使 Gate FAIL 仍 0，INCONCLUSIVE/不完整为 3。 |
| **L17** §27 L1448：gate 退出码 0 PASS /1 FAIL /2 config /3 INCONCLUSIVE /4 trust | runtime/rf-cli/src/main.rs:1001-1004 | 未实现 | rf gate 根本未进入执行分支，不能把 scan 内部的 gate preview 当独立命令实现。 |

## 逐声明核验表：§28 API Boundary

| 声明（规格原文位置） | 代码实证 | 判定 | 理由 |
| --- | --- | --- | --- |
| **A01** §28 L1461：POST /standards | contracts/http/ruleops-api-v1.yaml:46；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:346-353 | 部分一致 | 版本化契约存在，generic resource 路由可见；本轮未逐项跟踪 resourceType/GET 分派，不声称已完成请求级验证。 |
| **A02** §28 L1462：POST /coverage/analyze | contracts/http/ruleops-api-v1.yaml:48；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:327-344 | 一致 | 稳定 Domain API seam 有契约及分派，实际加 /v1 版本前缀；本声明只要求边界，不要求执行完整分析/生成任务。 |
| **A03** §28 L1464：POST /rules | contracts/http/ruleops-api-v1.yaml:50；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:346-353 | 部分一致 | 版本化契约存在，generic resource 路由可见；本轮未逐项跟踪 resourceType/GET 分派，不声称已完成请求级验证。 |
| **A04** §28 L1465：POST /rules/{id}/generate | contracts/http/ruleops-api-v1.yaml:52；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:327-344 | 一致 | 稳定 Domain API seam 有契约及分派，实际加 /v1 版本前缀；本声明只要求边界，不要求执行完整分析/生成任务。 |
| **A05** §28 L1466：POST /rules/{id}/evaluations | contracts/http/ruleops-api-v1.yaml:54；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:327-344 | 一致 | 稳定 Domain API seam 有契约及分派，实际加 /v1 版本前缀；本声明只要求边界，不要求执行完整分析/生成任务。 |
| **A06** §28 L1468：POST /releases | contracts/http/ruleops-api-v1.yaml:56；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:346-353 | 部分一致 | 版本化契约存在，generic resource 路由可见；本轮未逐项跟踪 resourceType/GET 分派，不声称已完成请求级验证。 |
| **A07** §28 L1469：POST /releases/{id}/promote | contracts/http/ruleops-api-v1.yaml:58；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:327-344 | 一致 | 稳定 Domain API seam 有契约及分派，实际加 /v1 版本前缀；本声明只要求边界，不要求执行完整分析/生成任务。 |
| **A08** §28 L1470：POST /releases/{id}/revoke | contracts/http/ruleops-api-v1.yaml:60；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:327-344 | 一致 | 稳定 Domain API seam 有契约及分派，实际加 /v1 版本前缀；本声明只要求边界，不要求执行完整分析/生成任务。 |
| **A09** §28 L1472：POST /scans | contracts/http/ruleops-api-v1.yaml:68；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:346-353 | 部分一致 | 版本化契约存在，generic resource 路由可见；本轮未逐项跟踪 resourceType/GET 分派，不声称已完成请求级验证。 |
| **A10** §28 L1473：GET  /scans/{id} | contracts/http/ruleops-api-v1.yaml:70；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:346-353 | 部分一致 | 版本化契约存在，generic resource 路由可见；本轮未逐项跟踪 resourceType/GET 分派，不声称已完成请求级验证。 |
| **A11** §28 L1474：GET  /findings | contracts/http/ruleops-api-v1.yaml:72；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:346-353 | 部分一致 | 版本化契约存在，generic resource 路由可见；本轮未逐项跟踪 resourceType/GET 分派，不声称已完成请求级验证。 |
| **A12** §28 L1476：POST /feedback | control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:41-42；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:356-361 | 部分一致 | 有 resource 类型与专门写入分支证据；完整路由/领域生命周期未逐项验证。 |
| **A13** §28 L1477：POST /exceptions | control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:41-42；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:356-361 | 部分一致 | 有 resource 类型与专门写入分支证据；完整路由/领域生命周期未逐项验证。 |

## 逐声明核验表：§29 Repository Layout

| 声明（规格原文位置） | 代码实证 | 判定 | 理由 |
| --- | --- | --- | --- |
| **R01** §29 L1486：顶层 specs/ | WSL 仓库根 ls -1 实测（见证据边界） | 一致 | 目录存在。 |
| **R02** §29 L1494：顶层 contracts/ | WSL 仓库根 ls -1 实测（见证据边界） | 一致 | 目录存在。 |
| **R03** §29 L1495：顶层 control-plane/ | WSL 仓库根 ls -1 实测（见证据边界） | 一致 | 目录存在。 |
| **R04** §29 L1501：顶层 runtime/ | WSL 仓库根 ls -1 实测（见证据边界） | 一致 | 目录存在。 |
| **R05** §29 L1506：顶层 workspace-resolvers/ | WSL 仓库根 ls -1 实测（见证据边界） | 一致 | 目录存在。 |
| **R06** §29 L1510：顶层 backend-hosts/ | WSL 仓库根 ls -1 实测（见证据边界） | 一致 | 目录存在。 |
| **R07** §29 L1512：顶层 adapters/ | WSL 仓库根 ls -1 实测（见证据边界） | 不一致 | 规格所列顶层目录不存在；仅布局漂移，不等于对应能力必然不存在。 |
| **R08** §29 L1515：顶层 templates/ | WSL 仓库根 ls -1 实测（见证据边界） | 不一致 | 规格所列顶层目录不存在；仅布局漂移，不等于对应能力必然不存在。 |
| **R09** §29 L1516：顶层 sandbox/ | WSL 仓库根 ls -1 实测（见证据边界） | 不一致 | 规格所列顶层目录不存在；仅布局漂移，不等于对应能力必然不存在。 |
| **R10** §29 L1517：顶层 ci/ | WSL 仓库根 ls -1 实测（见证据边界） | 不一致 | 规格所列顶层目录不存在；仅布局漂移，不等于对应能力必然不存在。 |
| **R11** §29 L1518：顶层 examples/ | WSL 仓库根 ls -1 实测（见证据边界） | 不一致 | 规格所列顶层目录不存在；仅布局漂移，不等于对应能力必然不存在。 |
| **R12** §29 L1519：顶层 docs/ | WSL 仓库根 ls -1 实测（见证据边界） | 一致 | 目录存在。 |
| **R13** §29 L1486：specs/ 子布局 ontology/standard/rule-intent/rule-ir/evidence/result/policy | find specs -mindepth 1 -maxdepth 1 -type d 实测 | 不一致 | 前四项存在，evidence/result/policy 不存在；实际还有 center/compatibility/coverage/pilot/security。 |
| **R14** §29 L1495：control-plane/ coverage/authoring/factory/verification/release | find control-plane -mindepth 1 -maxdepth 1 -type d 实测；runtime/release/src/lib.rs:1 | 不一致 | 前四项存在，release 不在 control-plane 下，实际有 runtime/release；另有 ontology/server。 |
| **R15** §29 L1501：runtime/ rf-cli/rf-runner/workspace-resolver/gate | find runtime -maxdepth 2 -type d 实测 | 不一致 | rf-cli/rf-runner/gate 存在，使用 resolver-client 而非所列 workspace-resolver 目录。 |
| **R16** §29 L1506：workspace-resolvers/ maven/gradle/javascript | find workspace-resolvers -mindepth 1 -maxdepth 1 -type d 实测 | 不一致 | 实际为 static/fake；不能从目录名称单独断言 Maven/Gradle 功能缺失。 |
| **R17** §29 L1510：backend-hosts/java 与 adapters/pmd-java、spotbugs | find backend-hosts -mindepth 1 -maxdepth 1 -type d；根 ls -1 | 部分一致 | Java host 存在，adapters 顶层不存在；PMD/SpotBugs 内部接入本轮未验证。 |

## 逐声明核验表：§29.1 Implementation Seams

| 声明（规格原文位置） | 代码实证 | 判定 | 理由 |
| --- | --- | --- | --- |
| **S01** §29.1 L1526：Java 21 模块化单体 | control-plane/factory/src/main/java/module-info.java；control-plane/server/src/main/java/module-info.java | 部分一致 | Java 模块文件存在；Java 21 版本和所有生命周期模块装配未完成核验。 |
| **S02** §29.1 L1527：rf 与 runner 为 Rust 且共享核心 | runtime/rf-cli/Cargo.toml:21-22；runtime/rf-cli/src/main.rs:9 | 一致 | CLI 直接依赖并调用 rf-runner/rf-gate。 |
| **S03** §29.1 L1528：Rust 拥有 Bundle/Signature/Cache/Planner/编排/Normalizer/Completeness/Baseline/Gate | runtime/rf-cli/Cargo.toml:9-22；runtime/gate/src/lib.rs:548-592 | 部分一致 | 对应 crate 与 Gate 逻辑可见；所有能力的唯一所有权未完整验证。 |
| **S04** §29.1 L1530：版本化跨进程 Backend Host Interface，不嵌入 Rust | contracts/proto/backend_host.proto；runtime/rf-cli/src/main.rs:125-137 | 部分一致 | 有独立协议文件及 host-client protocol/spawn/major-mismatch 错误分支；本轮未完整读取 protobuf 与 spawn 实现。 |
| **S05** §29.1 L1531：首个 Java Host 托管 PMD/SpotBugs，后续引擎独立 Adapter | backend-hosts/java/、backend-hosts/node-fake/ 实测 | 无法验证 | 存在 Java host，不足以确认 PMD/SpotBugs 两种后端均符合接口语义。 |
| **S06** §29.1 L1532：Rust 不暴露 PMD AST/SpotBugs Detector/Node/VM 类型 | contracts/proto/backend_host.proto；runtime/rf-runner/Cargo.toml:11 | 无法验证 | 自有 Contract 路径存在，但未完成公共类型面核查。 |
| **S07** §29.1 L1533：Factory Worker 独立进程/沙箱边界 | runtime/verification-worker/src/main.rs:45-69；runtime/verification-worker/src/lib.rs:255-280 | 部分一致 | 独立 Verification Worker 与 proof 验证存在；不能将其等同于 Candidate Factory Worker 全链路隔离。 |
| **S08** §29.1 L1534：首条 PMD 规则跑通 Rust Runtime→Java Host 纵切 | 未执行纵切或读取完整对应测试 | 无法验证 | 遵守禁止网络/cargo 与限时要求，不以目录/测试名替代运行证据。 |

## 问题清单（按严重度）

### P1 — ExceptionGrant 授权和实际豁免集合未在 trusted gate 边界闭合（E10/E12）
`trusted-ci::handoff` 以调用者给出的裸 fingerprint 集合进行豁免，再把另一份 grant digest 列表复制进 attestation；Gate 仅做集合 membership。已看到的 trusted-context 检查 repository/commit/policy/baseline/workload，但这些身份绑定不能替代 grant selector、有效期、撤回与签名的逐一授权校验。现有 grant registry 校验能力未在此消费点形成闭环。影响是 trusted CI API 调用者若误配或恶意传入豁免集合，可能对未获授权 Finding 签出通过结论；本轮未证明外部用户可直接控制该 API，也未制作利用样例。
### P1 — 规格 CLI 命令面大量缺失，尤其 `rf gate` 不存在（L01–L17）
当前 `runtime/rf-cli` 仅分派 `scan` 和新增 `registry`，规格列出的其余 14 个命令不受支持；`rf gate` 的五类退出码也无法兑现。`scan` 内部 preview 不能替代独立 gate 命令。若这是阶段性裁剪，需要显式同步版本范围，不应向使用者宣称完整 CLI 已交付。
### P2 — 安全控制证据有明显边界（T01–T13/C01–C14）
rootless OCI、禁网、配额、签名、工作负载身份与部分撤回路径有局部证据；模型无发布/签名权限、fixtures 绝不执行、PR 不能改 trusted gate、报告输出编码未找到充分实现证据。依赖 allowlist、SBOM/provenance 的真实生成和验证、ZIP 专门防护也未闭合。本项是验证/集成缺口，不将所有项归为已证实漏洞。
### P3 — 仓库布局与规格漂移（R01–R17）
缺少顶层 adapters/templates/sandbox/ci/examples；specs 少 evidence/result/policy；control-plane/release 移为 runtime/release；workspace-resolvers 只有 static/fake。功能可能重定位或尚未到交付阶段，不能以缺目录代替功能审计。
### P3 — 稳定 API 边界与完整领域执行应分别验收（A01–A13/F01/S01–S08）
HTTP 契约采用 /v1 前缀，若干路由落为资源/任务 seam；这不与“PoC 不要求服务化，但稳定 Domain API”矛盾。不能据接口/资源存在推断生成、评估、反馈迭代、生产 PMD 纵切已经端到端完成。

## 整改建议

1. 在 trusted Gate 的单一消费边界，将 RuleBundle/Resolved Policy 的允许 digest 解析成已验签、未撤回、有效且 scope 匹配的 grants，再从它们派生例外判断；不要让裸 fingerprint 与 grant digest 独立成为可签署输入。至少留一条负例：未引用/过期/撤回 grant 的 fingerprint 不能获得豁免。
2. 先确认当前版本承诺的 CLI 面；承诺保留则补真实命令分派和退出码验证，未承诺则修订规格/公开契约，不用 no-op 命令填齐列表。
3. 对没有执行证据的模型权限、可信 CI 不可变配置、Candidate 编译沙箱、fixture 不执行、report injection 分别补最小可复跑负例或实际部署策略证据；保留 fail-closed，不以布尔 proof 或组件名代替部署检验。
4. 更新布局图为实际模块归属，避免为目录对齐新增空层；将 API seam、领域任务执行、发布纵切分别定义验收证据。

## 证据边界

- 指定规格通过一次 `sed -n '1301,1538p'` 阅读；检索使用限界 `rg` 与编号片段。未读取此前域 7 审计。当前无可用语义检索工具，使用 exact anchors 的本地检索替代，未建索引。
- 根 `ls -1` 实测目录：backend-hosts、clients、contracts、control-plane、docs、runtime、scripts、specs、target、workspace-resolvers；另有 Cargo.lock/Cargo.toml/pom.xml/rf 及仓库文档。此处仅列目录，顶层文件不算新增模块。
- 子目录实测：control-plane={authoring,coverage,factory,ontology,server,verification}；workspace-resolvers={fake,static}；backend-hosts={fake,java,node-fake}；specs={center,compatibility,coverage,ontology,pilot,rule-intent,rule-ir,security,standard}。
- 未运行 `bash scripts/check.sh`：本任务禁止网络/Cargo、源码修改并设硬时限；因此不声明项目门禁全绿，不把静态审计当动态安全验证。仅校验报告结构、声明计数和规格锚点可解析。
- `runtime/rf-cli/src/main.rs` 含 NUL，普通 rg 报 binary match；随后用 `rg -a`/`nl` 读取分派及退出码，避免把二进制提示误判为空实现。
- 未验证：实际 CI 平台权限/受保护工作流，所有 Sandbox 调用链，依赖来源 allowlist，完整 SBOM/provenance 内容，ZIP 解压支持与限额，报告所有输出介质，cache 字节验证全链路，Java 21 实际构建/PMD-SpotBugs 运行，以及 Feedback→Release 全链路。相应行使用部分一致/无法验证。
- 安全严重度描述以本地可见边界为限：未声称获得生产凭据、成功攻击外部系统，或确认部署侧权限缺陷。
