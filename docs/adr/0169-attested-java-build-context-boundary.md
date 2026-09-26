# ADR-0169：ATTESTED Java 构建上下文边界

- 状态：Accepted
- 日期：2026-09-26
- 决策者：ERF Runtime / Java Backend Host

## 背景

Java 的类型解析与 SpotBugs 字节码分析不能从本地工作区猜测 classpath、编译输出或构建调用。仅把 java.symbol-resolution 宣布为能力，会把 SAFE_STATIC 的未知事实误当成可执行上下文。

## 决策

Workspace Resolution Contract v0 与 Backend Host Contract v0 在保持主版本不变的前提下，以新增字段传递以下闭合上下文：

- 有序 classpath_entries；
- 有序 build_output_roots；
- invocation_identity；
- 分别绑定上述列表的 SHA-256 摘要。

Resolver Client 校验路径文本、摘要和描述符整体摘要。Runner 对 ATTESTED_BUILD_EXPORT 的 Java 模块要求调用身份及两份摘要齐全；缺失时拒绝建立可信执行路径。SAFE_STATIC 不填充这些字段，也不由 Runner 或 Java Host 推断它们。

PMD 将已验证的 classpath 与编译输出根作为 auxiliary classpath，SpotBugs 将同一组路径作为 auxiliary classpath，并把调用身份纳入分析项目身份。Java Host 只接受仓库快照内、非符号链接的实际路径；上下文不完整或摘要不匹配时，单元失败而不是降级。

## 后果

- 当前 SAFE_STATIC 演示路径保持兼容，但不能宣称拥有真实依赖感知的 ATTESTED Java 能力。
- 未来的可信构建导出必须生成与字段规范一致的有序列表、摘要和调用身份，并把这些字段纳入 WorkspaceDescriptor 摘要与 WorkspaceAttestation。
- 协议扩展是 additive minor evolution；旧 Host/Resolver 可忽略未知字段，但 Runner 不会对缺失的 ATTESTED 上下文授予 Gate 权限。
