package io.ruleops.ide.actions;

// ERF 动作基类：统一取项目、配置、rf 调用与消息展示。
// 各动作只负责用 ThinClientCore 构造参数、调用 rf、以只读方式呈现结果。

import com.intellij.openapi.actionSystem.AnAction;
import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.fileChooser.FileChooser;
import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.ui.Messages;
import com.intellij.openapi.vfs.VirtualFile;
import io.ruleops.ide.ErfSettings;
import io.ruleops.ide.RfInvoker;

import java.util.List;

public abstract class AbstractErfAction extends AnAction {
    protected ErfSettings.State settings(Project project) {
        ErfSettings.State s = project.getService(ErfSettings.class).getState();
        return s != null ? s : new ErfSettings.State();
    }

    protected RfInvoker.Result runRf(Project project, List<String> args) throws Exception {
        ErfSettings.State s = settings(project);
        RfInvoker invoker = project.getService(RfInvoker.class);
        String base = project.getBasePath();
        return invoker.run(s.rfPath, args, base, s.requestTimeoutMs);
    }

    protected void info(Project project, String title, String body) {
        Messages.showInfoMessage(project, body, title);
    }

    protected void warn(Project project, String title, String body) {
        Messages.showWarningDialog(project, body, title);
    }

    protected boolean guardExit(Project project, String title, RfInvoker.Result r) {
        // exit 3 = 拒绝/PENDING/UNKNOWN，不作成功处理；exit 2 = 输入非法。
        String cls = io.ruleops.ide.core.ThinClientCore.classifyExitCode(r.exitCode);
        if (r.exitCode == 0) {
            return true;
        }
        warn(project, title, "rf 未成功（" + cls + ", exit=" + r.exitCode + "）：\n" + r.stderr);
        return false;
    }

    protected String prompt(Project project, String label) {
        return Messages.showInputDialog(project, label, "ERF", null);
    }

    protected VirtualFile pickJson(Project project, String title) {
        VirtualFile chosen = FileChooser.chooseFile(
            FileChooserDescriptorFactory.createSingleFileDescriptor("json").withTitle(title),
            project, project.getBaseDir());
        return chosen;
    }

    @Override
    public abstract void actionPerformed(AnActionEvent e);
}
