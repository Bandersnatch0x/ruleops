# ADR-0173: 确定性多规则 RuleBundle 打包边界

- Status: Accepted
- Date: 2026-09-30
- Scope: Phase 4 real-repository CI validation

## 背景

真实仓库的 Trusted 门禁已经能消费一个 Candidate，但每个规则都单独发布会重复下载、
重复初始化和重复执行同一批 Java 分析单元。这样既增加 CI 延迟，也让同一次变更的规则
集合难以作为一个不可变输入审计。

## 决策

新增 \`scripts/rule-bundle-pack.sh\` 作为发布前的确定性打包入口。它先复用一次 Maven
Factory classpath 编译所有输入 RuleIntent，再按 RuleIntent ID 排序，将每个 Candidate
中恰好一个的 PMD rule node 合成为一个 \`ruleset.xml\`。输出 manifest 使用
\`erf-rule-bundle-v1\`，记录 bundle ID、规则 ID/版本、模板、Candidate digest 和规则数量。

RuleBundle 的内容摘要沿用 Runner 的 canonical payload：只对排序后的可执行文件进行
长度前缀编码；\`manifest.json\` 不参与摘要，避免自引用。当前包只发布 \`ruleset.xml\`，不把
仓库源码、完整 Finding 或测试 fixture 带入运行时资产。tar/gzip 的成员排序、时间、owner
和 group 固定，同样输入产生相同发布字节。

旧的单规则 Candidate 仍由 Runner 原样接受；多规则包只是新增的 \`kind\`/manifest 形态，
不会改变现有单规则包的校验路径或 Trusted 门禁接口。多规则包的 \`intent.id\` 使用 bundle
ID，manifest 的 \`rules[]\` 才是规则集合的权威枚举。

## 权限与边界

打包不授予 Trusted、Advisory 或 Blocking authority；它只生成 Candidate/Shadow 输入。
只有目标 CI 的签名证据、身份校验和最终门禁步骤可以产生 Trusted 证明。规则集合仍需通过
真实仓库正例、隔离反例和回归扫描后，才能更新外部发布资产和 GitHub 仓库变量。

## 验证

\`\`\`text
bash scripts/rule-bundle-pack-check.sh .scratch/rule-bundle-pack-check
bash scripts/check.sh
\`\`\`

验收要求覆盖两条已治理 Java RuleIntent、两次独立打包的 manifest/ruleset/archive 字节
一致、两个 PMD rule node、每条规则的 Candidate digest，以及产物不含源码。
