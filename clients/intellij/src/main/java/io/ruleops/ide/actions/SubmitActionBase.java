package io.ruleops.ide.actions;

// 提交动作基类：要求显式 serverEndpoint（禁用自动发现），调用 `rf <kind> submit`，
// 只读渲染 SubmissionReceipt——三条虚假权威守卫必须恒 false，exit 3 视为 PENDING/UNKNOWN。

import com.google.gson.Gson;
import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.vfs.VirtualFile;
import io.ruleops.ide.ErfSettings;
import io.ruleops.ide.RfInvoker;
import io.ruleops.ide.core.ThinClientCore;

import java.util.List;
import java.util.Map;

public abstract class SubmitActionBase extends AbstractErfAction {
    private static final Gson GSON = new Gson();

    protected abstract String kind();

    @Override
    public void actionPerformed(AnActionEvent e) {
        Project project = e.getProject();
        if (project == null) return;
        ErfSettings.State s = settings(project);
        if (s.serverEndpoint == null || s.serverEndpoint.isEmpty()) {
            warn(project, "ERF · 提交", "未配置 serverEndpoint：提交默认关闭，需显式配置后重试。");
            return;
        }
        VirtualFile file = pickJson(project, "选择请求件（draft/request 产物 JSON）");
        if (file == null) return;
        try {
            List<String> args = ThinClientCore.buildSubmitArgs(
                kind(), file.getPath(), s.serverEndpoint, s.requestTimeoutMs, null);
            RfInvoker.Result r = runRf(project, args);
            if (r.exitCode == 3) {
                info(project, "ERF · 提交", ThinClientCore.statusLabel("PENDING")
                    + "\n（exit=3：拒绝/待定/未知，不作成功处理）\n" + r.stderr);
                return;
            }
            if (!guardExit(project, "ERF · 提交", r)) return;
            renderReceipt(project, r.stdout);
        } catch (IllegalArgumentException ex) {
            warn(project, "ERF · 参数缺失", String.valueOf(ex.getMessage()));
        } catch (Exception ex) {
            warn(project, "ERF · 提交失败", String.valueOf(ex.getMessage()));
        }
    }

    @SuppressWarnings("unchecked")
    private void renderReceipt(Project project, String stdout) {
        Map<String, Object> receipt;
        try {
            receipt = GSON.fromJson(stdout, Map.class);
        } catch (RuntimeException ex) {
            warn(project, "ERF · 回执", "无法解析回执，fail closed。");
            return;
        }
        if (receipt == null) {
            warn(project, "ERF · 回执", "空回执，fail closed。");
            return;
        }
        String status = String.valueOf(receipt.get("status"));
        ThinClientCore.ReceiptView view = ThinClientCore.renderReceiptGuards(
            status,
            Boolean.TRUE.equals(receipt.get("exceptionApproved")),
            Boolean.TRUE.equals(receipt.get("gateMutated")),
            Boolean.TRUE.equals(receipt.get("scmPublished")));
        if (!view.guardsHold) {
            warn(project, "ERF · 契约违背",
                "回执声称越权（" + String.join(", ", view.violations) + "），拒绝按成功处理。");
            return;
        }
        info(project, "ERF · 回执", view.status + "\n\n" + view.note);
    }
}
