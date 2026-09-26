package io.ruleops.ide.actions;

// 查询提交状态：要求显式 serverEndpoint 与 request-id 或 artifact-digest；
// 状态经 ThinClientCore.statusLabel 呈现，未知状态 fail closed。

import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import io.ruleops.ide.ErfSettings;
import io.ruleops.ide.RfInvoker;
import io.ruleops.ide.core.ThinClientCore;

import java.util.List;

public final class SubmissionStatusAction extends AbstractErfAction {
    @Override
    public void actionPerformed(AnActionEvent e) {
        Project project = e.getProject();
        if (project == null) return;
        ErfSettings.State s = settings(project);
        if (s.serverEndpoint == null || s.serverEndpoint.isEmpty()) {
            warn(project, "ERF · 状态", "未配置 serverEndpoint：查询默认关闭，需显式配置后重试。");
            return;
        }
        String requestId = prompt(project, "request-id（留空则改用 artifact-digest）");
        String digest = (requestId == null || requestId.isEmpty())
            ? prompt(project, "artifact-digest（sha256:...）") : null;
        try {
            List<String> args = ThinClientCore.buildSubmissionStatusArgs(
                s.serverEndpoint, requestId, digest, s.requestTimeoutMs);
            RfInvoker.Result r = runRf(project, args);
            if (r.exitCode == 3) {
                info(project, "ERF · 状态", ThinClientCore.statusLabel("UNKNOWN") + "\n" + r.stderr);
                return;
            }
            if (!guardExit(project, "ERF · 状态", r)) return;
            info(project, "ERF · 状态", r.stdout);
        } catch (IllegalArgumentException ex) {
            warn(project, "ERF · 参数缺失", String.valueOf(ex.getMessage()));
        } catch (Exception ex) {
            warn(project, "ERF · 状态查询失败", String.valueOf(ex.getMessage()));
        }
    }
}
