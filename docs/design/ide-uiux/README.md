# ERF IDE 插件双端 UI/UX 设计稿（存档）

**来源**：2026-10-05 由 Orca dispatched worker（antigravity/agy）经 **Stitch MCP** 生成，任务 `task_984abf2e1c42` / Dispatch `ctx_70fe67096a1d`，Run `run_c3a337dde377`。
**性质**：静态 HTML/PNG **示意稿**（render 自 Stitch 导出的 HTML，非真实 IDE 组件渲染）。生产于 `.scratch/ide-uiux-design/`，经用户决定持久化至本目录。

## 内容

- `design-spec.md` — 设计规范说明书（双端设计体系、17 场景矩阵、缺陷映射、治理语义、状态机）。
- `assets/` — 8 幅设计稿，每幅 `.png` + `.html`：
  - IntelliJ：`intellij_01_toolwindow_scenetree`、`intellij_02_scan_workbench`、`intellij_03_exceptions_feedback`、`intellij_04_settings_formbuilder`
  - VS Code：`vscode_01_treeview_commands`、`vscode_02_scan_workbench`、`vscode_03_governance_feedback_exception`、`vscode_04_registry_server_submissions`

## ⚠️ 重要：采纳状态（勿直接照稿实现）

本设计稿已经过**视觉验收 + 能力对比 + 防过度设计决策**，结论为 **Recirculate（不照稿重写）**。

- 决策矩阵（35 项设计点）与逐条理由、否决依据：见 [`../../refactor/ide-uiux-design-decision.md`](../../refactor/ide-uiux-design-decision.md)。
- 仅 **D1**（IntelliJ `TreeSpeedSearch`）与 **D2**（VS Code `aria-invalid` 主题错误边框）被采纳，已以原生最小改动落地（提交 `401ca33`）。
- 其余 33 项判为「已具备 / 过度设计 / 越界（超薄客户端职责，ADR-0166）/ 破坏输入与隐私契约」，**明确否决**。

## 证据边界

- 示意稿含越界内容（遥测默认开启、Daemon/Deploy 状态、审批台、精确进度百分比等），**不可作为需求来源**。
- 依赖 Tailwind CDN；IntelliJ 稿无 ARIA 标注、VS-04 无 `--vscode-*` token——**不是可复用实现**。
- 对齐/密度/一致性的判断基于缩略图与 HTML，无用户研究、无对比度实测、无读屏录制。
- **G1–G4 真机验收仍待人工**（见 `../../refactor/ide-thin-client-real-ide-acceptance-checklist.md`）。
