package io.ruleops.ide.actions;

// 起草 Feedback：调用 `rf feedback draft`，产出 PROPOSED 草稿（未提交）。

import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.vfs.VirtualFile;
import io.ruleops.ide.RfInvoker;
import io.ruleops.ide.core.ThinClientCore;

import java.util.List;

public final class DraftFeedbackAction extends AbstractErfAction {
    @Override
    public void actionPerformed(AnActionEvent e) {
        Project project = e.getProject();
        if (project == null) return;
        VirtualFile file = pickJson(project, "选择 Finding（feedback 输入 JSON）");
        if (file == null) return;
        String classification = prompt(project, "分类（true-fp / not-fp / scope-issue / documentation）");
        String reason = prompt(project, "理由");
        String actor = prompt(project, "操作者（actor）");
        try {
            List<String> args = ThinClientCore.buildFeedbackDraftArgs(
                file.getPath(), classification, reason, actor, null);
            RfInvoker.Result r = runRf(project, args);
            if (!guardExit(project, "ERF · Feedback 草稿", r)) return;
            info(project, "ERF · Feedback 草稿",
                ThinClientCore.statusLabel("PROPOSED") + "\n\n" + r.stdout);
        } catch (IllegalArgumentException ex) {
            warn(project, "ERF · 参数缺失", String.valueOf(ex.getMessage()));
        } catch (Exception ex) {
            warn(project, "ERF · Feedback 失败", String.valueOf(ex.getMessage()));
        }
    }
}
