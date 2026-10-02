package io.ruleops.ide.actions;

// 申请 Exception：调用 `rf exception request`，产出 REQUESTED 申请（未提交、未批准）。

import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.vfs.VirtualFile;
import io.ruleops.ide.RfInvoker;
import io.ruleops.ide.core.ThinClientCore;

import java.util.List;

public final class RequestExceptionAction extends AbstractErfAction {
    @Override
    public void actionPerformed(AnActionEvent e) {
        Project project = e.getProject();
        if (project == null) return;
        VirtualFile file = pickJson(project, "选择 Finding（exception 输入 JSON）");
        if (file == null) return;
        String scope = prompt(project, "范围（如 file / rule / occurrence）");
        String reason = prompt(project, "理由");
        String actor = prompt(project, "操作者（actor）");
        try {
            List<String> args = ThinClientCore.buildExceptionRequestArgs(
                file.getPath(), scope, reason, actor, null);
            RfInvoker.Result r = runRf(project, args);
            if (!guardExit(project, "ERF · Exception 申请", r)) return;
            info(project, "ERF · Exception 申请",
                ThinClientCore.statusLabel("REQUESTED") + "\n\n" + r.stdout);
        } catch (IllegalArgumentException ex) {
            warn(project, "ERF · 参数缺失", String.valueOf(ex.getMessage()));
        } catch (Exception ex) {
            warn(project, "ERF · Exception 失败", String.valueOf(ex.getMessage()));
        }
    }
}
