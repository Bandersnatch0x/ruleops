package io.ruleops.ide.actions;

// 展示 FindingView：仅呈现 digest-only 字段；隐私/形状不合规一律 fail-closed 拒绝。

import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.vfs.VirtualFile;

import java.nio.file.Path;
import java.util.Map;

public final class ShowFindingAction extends AbstractErfAction {
    @Override
    public void actionPerformed(AnActionEvent e) {
        Project project = e.getProject();
        if (project == null) return;
        VirtualFile file = pickJson(project, "选择 FindingView（digest-only JSON）");
        if (file == null) return;
        try {
            FindingViewLoader.Loaded loaded = FindingViewLoader.load(Path.of(file.getPath()));
            if (!loaded.check.ok) {
                warn(project, "ERF · FindingView 被拒绝", "拒绝原因：" + loaded.check.reason);
                return;
            }
            Map<String, Object> v = loaded.view;
            StringBuilder sb = new StringBuilder();
            sb.append("ruleId: ").append(v.get("ruleId")).append('\n');
            sb.append("findingId: ").append(v.get("findingId")).append('\n');
            sb.append("fingerprint: ").append(v.get("fingerprint")).append('\n');
            sb.append("message: ").append(v.get("message")).append('\n');
            sb.append("location: ").append(v.get("location"));
            info(project, "ERF · Finding", sb.toString());
        } catch (Exception ex) {
            warn(project, "ERF · 读取失败", String.valueOf(ex.getMessage()));
        }
    }
}
