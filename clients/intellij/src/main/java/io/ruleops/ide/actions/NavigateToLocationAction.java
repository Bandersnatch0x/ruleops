package io.ruleops.ide.actions;

// 导航到 Finding 位置：双重路径门禁——先 ThinClientCore.validateRelativePath，
// 再确认解析后的绝对路径仍落在项目根内，任一不过即拒绝，绝不打开工作区外文件。

import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.fileEditor.OpenFileDescriptor;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.vfs.LocalFileSystem;
import com.intellij.openapi.vfs.VirtualFile;
import io.ruleops.ide.core.ThinClientCore;

import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.util.Map;

public final class NavigateToLocationAction extends AbstractErfAction {
    @Override
    public void actionPerformed(AnActionEvent e) {
        Project project = e.getProject();
        if (project == null) return;
        String base = project.getBasePath();
        if (base == null) {
            warn(project, "ERF · 导航", "无项目根，拒绝导航。");
            return;
        }
        VirtualFile file = pickJson(project, "选择 FindingView（digest-only JSON）");
        if (file == null) return;
        try {
            FindingViewLoader.Loaded loaded = FindingViewLoader.load(Path.of(file.getPath()));
            if (!loaded.check.ok) {
                warn(project, "ERF · FindingView 被拒绝", "拒绝原因：" + loaded.check.reason);
                return;
            }
            @SuppressWarnings("unchecked")
            Map<String, Object> loc = (Map<String, Object>) loaded.view.get("location");
            String rel = String.valueOf(loc.get("path"));
            ThinClientCore.Check pathCheck = ThinClientCore.validateRelativePath(rel);
            if (!pathCheck.ok) {
                warn(project, "ERF · 导航被拒绝", "路径不安全：" + pathCheck.reason);
                return;
            }
            Path root = Path.of(base).toAbsolutePath().normalize();
            Path target = root.resolve(rel).normalize();
            if (!target.startsWith(root)) {
                warn(project, "ERF · 导航被拒绝", "目标逃逸项目根，拒绝。");
                return;
            }
            target = resolveSafeWorkspaceFile(root, target);
            VirtualFile vf = LocalFileSystem.getInstance().refreshAndFindFileByNioFile(target);
            if (vf == null) {
                warn(project, "ERF · 导航", "目标文件不存在：" + rel);
                return;
            }
            int line = ((Number) loc.get("line")).intValue() - 1;
            int col = ((Number) loc.get("column")).intValue() - 1;
            new OpenFileDescriptor(project, vf, Math.max(0, line), Math.max(0, col)).navigate(true);
        } catch (Exception ex) {
            warn(project, "ERF · 导航失败", String.valueOf(ex.getMessage()));
        }
    }

    /** Reject every symlink component, then bind the existing file to the real project root. */
    private static Path resolveSafeWorkspaceFile(Path root, Path target) throws Exception {
        if (!Files.isDirectory(root, LinkOption.NOFOLLOW_LINKS)
                || !Files.isRegularFile(target, LinkOption.NOFOLLOW_LINKS)
                || Files.isSymbolicLink(target)) {
            throw new IllegalArgumentException("target is not a regular non-symlink file");
        }
        Path cursor = root;
        for (Path component : root.relativize(target)) {
            cursor = cursor.resolve(component);
            if (Files.isSymbolicLink(cursor)) {
                throw new IllegalArgumentException("SYMLINK_COMPONENT");
            }
        }
        Path realRoot = root.toRealPath();
        Path realTarget = target.toRealPath();
        if (!realTarget.startsWith(realRoot)) {
            throw new IllegalArgumentException("REAL_TARGET_ESCAPES_WORKSPACE");
        }
        return target;
    }
}
