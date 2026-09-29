// 评审当前所有代码实现 — read-only review fan-out over implementation slices,
// independent confirmation of every finding, a cross-slice synthesis, and the
// repo's own delivery gate as the deterministic close.

interface Finding {
  /** Workspace-relative 路径，带行号更佳，如 "runtime/gate/src/lib.rs:120"。 */
  where: string;
  /** 一句话：这是什么问题（不是怎么修）。 */
  what: string;
  /** 你看到的证据：读到的代码行 / 判定的理由。 */
  evidence: string;
  /** 影响程度；"high" 只留给数据丢失、崩溃或明显错误结果。 */
  severity: "low" | "medium" | "high";
  /** 所属切面（评审者原样回填）。 */
  slice: string;
}

interface Confirmation {
  /** 原样回传的 Finding（confirm 基于它复核）。 */
  finding: Finding;
  /** "verified" 你独立复核后问题确实存在；"refuted" 复核后问题不成立或不准确。 */
  verdict: "verified" | "refuted";
  /** 复核依据：读到的代码，或没复现出来的原因。 */
  basis: string;
}

interface Synthesis {
  /** 给用户的总评：两三条最重要结论。 */
  conclusion: string;
  /** 有没有跨切面（执行核心 vs 创作簇 vs 契约）的系统性问题；没有则说没有。 */
  systemicIssues: string;
  /** 面向用户的优先级建议（可选）。 */
  recommendation: string;
}

const REVIEW_RULES =
  "你是只读代码评审员。规则：绝不编辑任何文件，绝不运行会修改仓库的命令；" +
  "只报告你实际读到的代码行支持的问题，绝不猜测你没读到的东西；" +
  "该仓对外定位已收窄为「可信规则执行层」：核心卖点是 Completeness、三态 Gate、" +
  "签名执行证明取代「0 告警即通过」——评审时优先核对实现是否与这个边界一致、" +
  "有没有把 SHADOW/LOCAL_UNATTESTED 误当成可信执行、有没有假证据/假装可信；" +
  "每个子项给出 3-8 条最重要发现（没有实质问题就如实返回空数组），不要凑数；用中文回答。";

const CONFIRM_RULES =
  "你是独立复核员。规则：绝不编辑任何文件。基于给你的 Finding 亲自去读对应代码，" +
  "判断问题是否真实存在、severity/where 是否准确；不要轻信 Finding 的说法；" +
  "无法复现或代码并不支持时判 refuted 并说明原因；用中文回答。";

const SLICES: string[] = [
  "runtime/rf-cli",
  "runtime/rf-runner",
  "runtime/gate 与 runtime/rule-bundle 与 runtime/bundle-delivery",
  "runtime/trust 与 runtime/trust-state 与 runtime/trusted-context 与 runtime/trusted-ci 与 runtime/authority 与 runtime/release 与 runtime/release-registry 与 runtime/governance-registry",
  "runtime/scan-plan 与 runtime/scan-output 与 runtime/project-config 与 runtime/affected-scope 与 runtime/git-scope 与 runtime/incremental-baseline",
  "runtime/status 与 runtime/thin-client 与 runtime/feedback",
  "runtime/pilot-metrics 与 runtime/pilot-observer 与 runtime/pilot-evidence",
  "runtime/fast-decision 与 runtime/unified-result 与 runtime/packs 与 runtime/bootstrap 与 runtime/sandbox 与 runtime/bundle-cache 与 runtime/host-client 与 runtime/resolver-client 与 runtime/conformance 与 runtime/framing",
  "backend-hosts/fake 与 backend-hosts/java 与 workspace-resolvers/fake 与 workspace-resolvers/static",
  "control-plane/authoring 与 control-plane/factory 与 control-plane/coverage",
  "control-plane/ontology 与 control-plane/server 与 control-plane/verification",
  "clients/vscode 与 clients/intellij",
  "contracts/proto 与 contracts/http 与 contracts/rust 以及执行核心/创作簇边界的跨切面一致性",
];

phase("逐切面评审实现并即时复核发现");
const confirmed: Confirmation[] = (
  await Promise.all(
    SLICES.map(async (slice) => {
      const review = await agent(`评审员-${slice}`, REVIEW_RULES).ask<Finding[]>(
        `评审这个实现切面：${slice}。请彻底、诚实，重点核对与「可信规则执行层」边界的一致性。` +
          "返回 Finding 数组；slice 字段填本切面名。",
      );
      return Promise.all(
        review.map((f, i) =>
          agent(`复核员-${slice}-${i}`, CONFIRM_RULES).ask<Confirmation>(
            `复核这条发现：${JSON.stringify(f)}`,
          ),
        ),
      );
    }),
  )
).flat();

const verifiedFindings: Finding[] = [];
const unconfirmedFindings: Finding[] = [];
const refutedFindings: Finding[] = [];
for (const c of confirmed) {
  if (c.verdict === "verified") verifiedFindings.push(c.finding);
  else if (c.verdict === "refuted") refutedFindings.push(c.finding);
  else unconfirmedFindings.push(c.finding);
  report({ ...c.finding, status: c.verdict, basis: c.basis });
}

phase("汇总跨切面结论");
const synth = await agent(
  "汇总员",
  "你是资深架构评审汇总者。基于下面已确认的发现做跨切面归纳，识别系统性问题；不要新增你没看到证据的判断；用中文回答。",
).ask<Synthesis>(
  `已确认发现（经独立复核 verified）：\n${JSON.stringify(verifiedFindings)}\n\n` +
    `未确认发现（复核未能证实，保留供人工判断）：\n${JSON.stringify(unconfirmedFindings)}\n\n` +
    "请产出总评、跨切面系统性问题（如有）、以及面向用户的优先级建议。",
);

phase("运行仓库交付门禁确认全绿");
const gate = await world.run("bash", ["scripts/check.sh"], { timeoutMs: 1_200_000 });
const gateGreen = gate.exitCode === 0;
log(`交付门禁退出码 ${gate.exitCode}${gateGreen ? "（全绿）" : "（出现失败，stderr 尾部见下）"}`);
if (!gateGreen) log(gate.stderr.slice(-2000));

phase("产出评审报告");
const lines: string[] = [];
lines.push(`# 全量代码实现评审报告`);
lines.push(``);
lines.push(`## 总评`);
lines.push(synth.conclusion);
lines.push(``);
lines.push(`## 跨切面系统性问题`);
lines.push(synth.systemicIssues);
lines.push(``);
lines.push(`## 优先级建议`);
lines.push(synth.recommendation);
lines.push(``);
lines.push(`## 交付门禁`);
lines.push(`- \`bash scripts/check.sh\` 退出码 ${gate.exitCode}${gateGreen ? "（全绿）" : "（失败）"}`);
lines.push(``);
lines.push(`## 已确认发现（${verifiedFindings.length}）`);
for (const f of verifiedFindings) lines.push(`- [${f.severity}] \`${f.where}\` — ${f.what}（证据：${f.evidence}）`);
if (verifiedFindings.length === 0) lines.push(`- （无）`);
lines.push(``);
lines.push(`## 未确认发现（复核未证实，保留供人工判断，${unconfirmedFindings.length}）`);
for (const f of unconfirmedFindings) lines.push(`- [${f.severity}] \`${f.where}\` — ${f.what}`);
if (unconfirmedFindings.length === 0) lines.push(`- （无）`);
lines.push(``);
lines.push(`## 复核推翻的发现（${refutedFindings.length}）`);
for (const f of refutedFindings) lines.push(`- \`${f.where}\` — ${f.what}`);
if (refutedFindings.length === 0) lines.push(`- （无）`);

await artifact.markdown("report", lines.join("\n"), {
  title: "全量代码实现评审报告",
  description: `13 个切面逐一评审 + 独立复核；已确认 ${verifiedFindings.length} 条 / 未确认 ${unconfirmedFindings.length} 条 / 推翻 ${refutedFindings.length} 条。`,
  primary: true,
});

const result = {
  conclusion:
    synth.conclusion +
    `${verifiedFindings.length > 0 ? ` 发现 ${verifiedFindings.length} 条已确认问题，详见报告。` : " 未发现实质性问题。"}` +
    `${gateGreen ? "" : " 交付门禁本次复跑未全绿，需关注。"}`,
  findings: verifiedFindings.map((f) => ({
    where: f.where,
    what: f.what,
    evidence: f.evidence,
    status: "verified" as const,
    severity: f.severity,
  })),
  verified: [
    `13 个实现切面逐一评审（每条发现均经独立复核员复核）`,
    `交付门禁 bash scripts/check.sh 退出码 ${gate.exitCode}`,
  ],
  notCovered: [
    "性能基准未跑（属 LIVE_SHADOW/吞吐真值，人工前沿）",
    "真实 SCM / 真实生产 LIVE_SHADOW（人工前沿，非本评审范围）",
  ],
};
return result;
