package io.ruleops.ide.actions;

// 解释 Finding：调用 `rf explain --format text`，解析 key=value 后只读呈现。

import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.vfs.VirtualFile;
import io.ruleops.ide.RfInvoker;
import io.ruleops.ide.core.ThinClientCore;

import java.util.List;
import java.util.Map;

public final class ExplainFindingAction extends AbstractErfAction {
    @Override
    public void actionPerformed(AnActionEvent e) {
        Project project = e.getProject();
        if (project == null) return;
        VirtualFile file = pickJson(project, "选择 Finding（explain 输入 JSON）");
        if (file == null) return;
        try {
            ThinClientCore.ExplainOpts opts = new ThinClientCore.ExplainOpts();
            opts.format = "text";
            List<String> args = ThinClientCore.buildExplainArgs(file.getPath(), opts);
            RfInvoker.Result r = runRf(project, args);
            if (!guardExit(project, "ERF · 解释", r)) return;
            Map<String, String> parsed = ThinClientCore.parseExplainText(r.stdout);
            StringBuilder sb = new StringBuilder();
            parsed.forEach((k, v) -> sb.append(k).append(": ").append(v).append('\n'));
            info(project, "ERF · 解释", sb.length() == 0 ? r.stdout : sb.toString());
        } catch (Exception ex) {
            warn(project, "ERF · 解释失败", String.valueOf(ex.getMessage()));
        }
    }
}
