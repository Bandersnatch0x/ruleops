package io.ruleops.ide;

import com.google.gson.*;
import com.intellij.openapi.Disposable;
import com.intellij.openapi.fileChooser.FileChooser;
import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory;
import com.intellij.openapi.fileEditor.OpenFileDescriptor;
import com.intellij.openapi.options.ShowSettingsUtil;
import com.intellij.openapi.progress.ProgressIndicator;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.util.Key;
import com.intellij.openapi.vfs.LocalFileSystem;
import com.intellij.openapi.wm.ToolWindow;
import com.intellij.openapi.wm.ToolWindowFactory;
import com.intellij.openapi.wm.ToolWindowManager;
import com.intellij.ui.content.Content;
import com.intellij.ui.content.ContentFactory;
import io.ruleops.ide.actions.FindingViewLoader;
import io.ruleops.ide.core.ThinClientCore;
import org.jetbrains.annotations.NotNull;
import javax.swing.*;
import javax.swing.tree.*;
import java.awt.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.List;
import java.util.Map;

public final class ErfToolWindowFactory implements ToolWindowFactory {
    private static final Key<Panel> PANEL = Key.create("erf.toolWindow.panel");
    public enum Scene {
        SHOW("显示 Finding", "scan_findings", null), NAVIGATE("跳转到 Finding", "scan_findings", null),
        EXPLAIN("解释 Finding", "scan_findings", "rf.explain"),
        FEEDBACK("起草反馈", "feedback_exceptions", "rf.feedback.draft"),
        EXCEPTION("申请例外（当前文件）", "feedback_exceptions", "rf.exception.request"),
        SUBMIT_FEEDBACK("提交反馈", "feedback_exceptions", "rf.feedback.submit"),
        SUBMIT_EXCEPTION("提交例外", "feedback_exceptions", "rf.exception.submit"),
        STATUS("查询提交状态", "feedback_exceptions", "rf.submission.status");
        final String title, section, cli;
        Scene(String title, String section, String cli) { this.title = title; this.section = section; this.cli = cli; }
        boolean server() { return this == SUBMIT_FEEDBACK || this == SUBMIT_EXCEPTION || this == STATUS; }
        @Override public String toString() { return title; }
    }
    @Override public void createToolWindowContent(@NotNull Project project, @NotNull ToolWindow toolWindow) {
        Panel panel = new Panel(project);
        project.putUserData(PANEL, panel);
        Content content = ContentFactory.getInstance().createContent(panel, "", false);
        content.setDisposer(panel);
        toolWindow.getContentManager().addContent(content);
    }
    public static void open(Project project, Scene scene) {
        ToolWindow window = ToolWindowManager.getInstance(project).getToolWindow("ERF");
        if (window == null) throw new IllegalStateException("ERF_TOOL_WINDOW_NOT_REGISTERED");
        window.show(() -> {
            Panel panel = project.getUserData(PANEL);
            if (panel == null) throw new IllegalStateException("ERF_TOOL_WINDOW_NOT_INITIALIZED");
            panel.select(scene);
        });
    }
    public static void settingsChanged(Project project) {
        Panel panel = project.getUserData(PANEL);
        if (panel != null) { panel.generation++; if (panel.job != null) panel.job.cancel(); else panel.refresh(); }
    }
    private record Unavailable(String template) { @Override public String toString() { return template + " · 不可用"; } }
    private record Execution(String detail, Path target, int line, int column) {
        Execution(String detail) { this(detail, null, 0, 0); }
    }
    static final class Panel extends JPanel implements Disposable {
        private final Project project;
        private final JTree tree;
        private final JTextArea context = new JTextArea();
        private final JTextArea output = new JTextArea();
        private final JLabel status = new JLabel();
        private final JPanel form = new JPanel();
        private final JTextField file = new JTextField();
        private final JTextField actor = new JTextField();
        private final JTextArea reason = new JTextArea(3, 20);
        private final JTextField requestId = new JTextField();
        private final JTextField digest = new JTextField();
        private final JComboBox<String> classification = new JComboBox<>(new String[]{"true-fp", "not-fp", "scope-issue", "documentation"});
        private final JButton execute = new JButton("执行所选场景");
        private final JButton cancel = new JButton("取消");
        private final JButton pick = new JButton("选择 JSON 文件");
        private final JButton refresh = new JButton("刷新上下文");
        private final DefaultMutableTreeNode runs = new DefaultMutableTreeNode("运行记录");
        private Scene scene = Scene.SHOW;
        private RfInvoker.Job job;
        private boolean disposed;
        private long generation;

        Panel(Project project) {
            super(new BorderLayout(8, 8)); this.project = project;
            DefaultMutableTreeNode root = new DefaultMutableTreeNode("ERF");
            try (InputStream stream = ErfToolWindowFactory.class.getResourceAsStream("/ide-command-map.json")) {
                if (stream == null) throw new IllegalStateException("MISSING_IDE_COMMAND_MAP");
                JsonObject map = JsonParser.parseReader(new InputStreamReader(stream, StandardCharsets.UTF_8)).getAsJsonObject();
                for (Map.Entry<String, JsonElement> entry : map.getAsJsonObject("sections").entrySet()) {
                    DefaultMutableTreeNode section = new DefaultMutableTreeNode(entry.getValue().getAsString()); root.add(section);
                    for (Scene s : Scene.values()) if (s.section.equals(entry.getKey())) section.add(new DefaultMutableTreeNode(s));
                    for (JsonElement element : map.getAsJsonArray("commands")) {
                        JsonObject command = element.getAsJsonObject();
                        boolean supported = java.util.Arrays.stream(Scene.values()).anyMatch(s -> command.get("commandId").getAsString().equals(s.cli));
                        if (command.get("section").getAsString().equals(entry.getKey()) && !supported)
                            section.add(new DefaultMutableTreeNode(new Unavailable(command.get("commandTemplate").getAsString())));
                    }
                }
            } catch (IOException error) { throw new IllegalStateException("IDE_COMMAND_MAP_READ_FAILED", error); }
            root.add(runs); tree = new JTree(root); tree.setRootVisible(false);
            JPanel right = new JPanel(new BorderLayout(6, 6));
            context.setEditable(false); context.setLineWrap(true); context.setWrapStyleWord(true);
            right.add(context, BorderLayout.NORTH);
            form.setLayout(new BoxLayout(form, BoxLayout.Y_AXIS));
            output.setEditable(false); output.setLineWrap(true); output.setWrapStyleWord(true);
            JPanel details = new JPanel(new BorderLayout(6, 6)); details.add(form, BorderLayout.NORTH);
            details.add(new JScrollPane(output), BorderLayout.CENTER); right.add(details, BorderLayout.CENTER);
            JPanel buttons = new JPanel(new FlowLayout(FlowLayout.LEADING));
            JButton settings = new JButton("ERF 设置");
            for (JButton button : List.of(execute, cancel, refresh, settings)) buttons.add(button);
            JPanel footer = new JPanel(new BorderLayout()); footer.add(status, BorderLayout.NORTH); footer.add(buttons, BorderLayout.SOUTH);
            right.add(footer, BorderLayout.SOUTH);
            JSplitPane split = new JSplitPane(JSplitPane.VERTICAL_SPLIT, new JScrollPane(tree), right);
            split.setResizeWeight(.3); add(split);
            file.setEditable(false);
            pick.addActionListener(e -> {
                var chosen = FileChooser.chooseFile(FileChooserDescriptorFactory.createSingleFileDescriptor("json"), project, null);
                if (chosen != null) file.setText(chosen.getPath());
            });
            tree.addTreeSelectionListener(e -> {
                if (job != null) return;
                var node = (DefaultMutableTreeNode) tree.getLastSelectedPathComponent();
                if (node != null && node.getUserObject() instanceof Scene s) setScene(s);
                else if (node != null && node.getUserObject() instanceof Unavailable unavailable) {
                    scene = null; form.removeAll(); form.revalidate(); form.repaint();
                    setState("不可用", "阶段 1 尚未提供此命令的参数面板，不执行模板或自动准备安装资产。\n" + unavailable.template()); execute.setEnabled(false);
                }
            });
            execute.addActionListener(e -> execute()); cancel.addActionListener(e -> { if (job != null) job.cancel(); });
            refresh.addActionListener(e -> refresh());
            settings.addActionListener(e -> ShowSettingsUtil.getInstance().showSettingsDialog(project, ErfConfigurable.class));
            select(Scene.SHOW);
        }
        void select(Scene next) {
            if (job != null) return;
            var nodes = ((DefaultMutableTreeNode) tree.getModel().getRoot()).depthFirstEnumeration();
            while (nodes.hasMoreElements()) {
                var node = (DefaultMutableTreeNode) nodes.nextElement();
                if (node.getUserObject() == next) {
                    TreePath path = new TreePath(node.getPath()); tree.setSelectionPath(path); tree.scrollPathToVisible(path); break;
                }
            }
            setScene(next);
        }
        private void setScene(Scene next) {
            if (scene == null || scene.server() != next.server()) file.setText("");
            scene = next; generation++; form.removeAll();
            form.add(new JLabel(next.title));
            if (next != Scene.STATUS) { form.add(pick); form.add(file); }
            if (next == Scene.FEEDBACK) field("分类", classification);
            if (next == Scene.FEEDBACK || next == Scene.EXCEPTION) {
                field("提交者（本人确认）", actor); field("理由", new JScrollPane(reason));
            }
            if (next == Scene.EXCEPTION) form.add(new JLabel("scope = 已验证 Finding 当前文件；本阶段不扩范围"));
            if (next == Scene.STATUS) { field("requestId", requestId); field("artifactDigest（requestId 留空时使用）", digest); }
            form.revalidate(); form.repaint(); refresh();
        }
        private void field(String text, JComponent input) { JLabel label = new JLabel(text); label.setLabelFor(input); form.add(label); form.add(input); }
        private void refresh() {
            if (job != null) return;
            ErfSettings.State c = project.getService(ErfSettings.class).snapshot();
            context.setText("工作区：" + project.getBasePath() + "\nServer：" + (c.serverEndpoint.isBlank() ? "未配置" : "已配置（由 CLI 校验连接）") + "\n提交 ≠ 批准 ≠ Gate PASS；不写入 SCM。");
            cancel.setEnabled(false);
            boolean ready = scene != null && project.getBasePath() != null && !c.rfPath.isBlank() && c.requestTimeoutMs > 0 && (!scene.server() || !c.serverEndpoint.isBlank());
            execute.setEnabled(ready);
            if (scene == null) setState("不可用", "阶段 1 尚未提供此场景参数面板。");
            else setState(ready ? "就绪" : "未配置", ready ? "rf 可用性将在显式执行时确认。" : "请配置工作区、rf 路径、正整数超时及所需 Server endpoint。");
        }
        private void setState(String phase, String detail) { status.setText(phase); output.setText(detail); }
        private void busy(boolean value) {
            execute.setEnabled(!value); cancel.setEnabled(value); tree.setEnabled(!value); pick.setEnabled(!value); refresh.setEnabled(!value);
            for (JComponent input : List.of(actor, reason, requestId, digest, classification)) input.setEnabled(!value);
        }
        private void execute() {
            if (job != null || scene == null) return;
            refresh(); if (!execute.isEnabled()) return;
            Scene selected = scene;
            ErfSettings.State c = project.getService(ErfSettings.class).snapshot();
            String base = project.getBasePath(), input = file.getText(), who = actor.getText(), why = reason.getText();
            String cls = String.valueOf(classification.getSelectedItem()), id = requestId.getText(), artifact = digest.getText();
            long version = generation;
            RfInvoker invoker = project.getService(RfInvoker.class);
            busy(true); setState("运行中", selected.title);
            job = invoker.submit(project, indicator -> perform(invoker, indicator, selected, c, base, input, who, why, cls, id, artifact),
                result -> {
                    job = null; if (disposed) return; busy(false);
                    if (version != generation) { refresh(); return; }
                    if (result.target() != null) {
                        var target = LocalFileSystem.getInstance().refreshAndFindFileByNioFile(result.target());
                        if (target == null) { finish("失败", "目标已消失"); return; }
                        new OpenFileDescriptor(project, target, result.line(), result.column()).navigate(true);
                    }
                    finish("已完成（非批准）", result.detail());
                }, error -> {
                    job = null; if (disposed) return; busy(false);
                    if (version != generation) { refresh(); return; }
                    finish(error instanceof RfInvoker.UnavailableException ? "不可用" : "失败", error.toString());
                }, () -> {
                    job = null; if (disposed) return; busy(false);
                    if (version != generation) { refresh(); return; }
                    finish("已取消", "已停止；远端请求可能已经接收，取消不代表回滚，请查询状态。");
                });
        }
        private void finish(String phase, String detail) {
            setState(phase, detail);
            runs.insert(new DefaultMutableTreeNode(scene.title + " · " + phase), 0);
            if (runs.getChildCount() > 20) runs.remove(20);
            ((DefaultTreeModel) tree.getModel()).reload(runs);
        }
        @Override public void dispose() { disposed = true; generation++; if (job != null) job.cancel(); project.putUserData(PANEL, null); }
    }
    private static Execution perform(RfInvoker invoker, ProgressIndicator indicator, Scene scene, ErfSettings.State c,
                                     String base, String file, String actor, String reason, String classification, String requestId, String digest) throws Exception {
        indicator.checkCanceled();
        FindingViewLoader.Loaded loaded = null;
        if (!scene.server()) {
            if (file.isBlank()) throw new IllegalArgumentException("请选择 FindingView JSON");
            loaded = FindingViewLoader.load(Path.of(file));
            if (!loaded.check.ok) throw new IllegalArgumentException("FindingView 被拒绝：" + loaded.check.reason);
        }
        if (scene == Scene.SHOW) return new Execution(new Gson().toJson(loaded.view));
        if (scene == Scene.NAVIGATE) {
            @SuppressWarnings("unchecked") Map<String, Object> location = (Map<String, Object>) loaded.view.get("location");
            String relative = String.valueOf(location.get("path"));
            ThinClientCore.Check check = ThinClientCore.validateRelativePath(relative);
            if (!check.ok) throw new IllegalArgumentException(check.reason);
            Path target = resolveSafeWorkspaceFile(Path.of(base).toAbsolutePath().normalize(), relative);
            return new Execution(relative, target, ((Number) location.get("line")).intValue() - 1, ((Number) location.get("column")).intValue() - 1);
        }
        List<String> args;
        switch (scene) {
            case EXPLAIN -> { var opts = new ThinClientCore.ExplainOpts(); opts.format = "text"; args = ThinClientCore.buildExplainArgs(file, opts); }
            case FEEDBACK -> args = ThinClientCore.buildFeedbackDraftArgs(file, classification, reason, actor, null);
            case EXCEPTION -> {
                @SuppressWarnings("unchecked") Map<String, Object> location = (Map<String, Object>) loaded.view.get("location");
                args = ThinClientCore.buildExceptionRequestArgs(file, String.valueOf(location.get("path")), reason, actor, null);
            }
            case SUBMIT_FEEDBACK, SUBMIT_EXCEPTION -> {
                if (file.isBlank()) throw new IllegalArgumentException("请选择已保存请求 JSON");
                args = ThinClientCore.buildSubmitArgs(scene == Scene.SUBMIT_FEEDBACK ? "feedback" : "exception", file, c.serverEndpoint, c.requestTimeoutMs, null);
            }
            case STATUS -> args = ThinClientCore.buildSubmissionStatusArgs(c.serverEndpoint, requestId, digest, c.requestTimeoutMs);
            default -> throw new IllegalStateException("UNAVAILABLE_SCENE");
        }
        RfInvoker.Result result = invoker.run(c.rfPath, args, base, c.requestTimeoutMs, indicator);
        if (result.exitCode() != 0) throw new IllegalStateException(ThinClientCore.classifyExitCode(result.exitCode()) + " · exit=" + result.exitCode() + "\n" + result.stderr());
        String detail = result.stdout();
        if (scene.server()) {
            JsonElement parsed = JsonParser.parseString(detail);
            if (!parsed.isJsonObject()) throw new IllegalArgumentException("INVALID_RECEIPT");
            JsonObject receipt = parsed.getAsJsonObject();
            var rendered = ThinClientCore.renderReceiptGuards(receipt.has("status") ? receipt.get("status").getAsString() : "UNKNOWN",
                isTrue(receipt, "exceptionApproved"), isTrue(receipt, "gateMutated"), isTrue(receipt, "scmPublished"));
            if (!rendered.guardsHold) throw new IllegalStateException("回执违反权威守卫：" + rendered.violations);
            detail = rendered.status + "\n" + rendered.note + "\n" + detail;
        }
        if (scene == Scene.FEEDBACK) detail = ThinClientCore.statusLabel("PROPOSED") + "\n" + detail;
        if (scene == Scene.EXCEPTION) detail = ThinClientCore.statusLabel("REQUESTED") + "\n" + detail;
        return new Execution("exit=0\n" + detail);
    }
    private static boolean isTrue(JsonObject object, String name) {
        return object.has(name) && object.get(name).isJsonPrimitive() && object.get(name).getAsJsonPrimitive().isBoolean() && object.get(name).getAsBoolean();
    }
    private static Path resolveSafeWorkspaceFile(Path root, String relative) throws Exception {
        Path target = root.resolve(relative).normalize();
        if (!target.startsWith(root)) throw new IllegalArgumentException("TARGET_ESCAPES_WORKSPACE");
        if (!Files.isDirectory(root, LinkOption.NOFOLLOW_LINKS) || !Files.isRegularFile(target, LinkOption.NOFOLLOW_LINKS) || Files.isSymbolicLink(target))
            throw new IllegalArgumentException("target is not a regular non-symlink file");
        Path cursor = root;
        for (Path part : root.relativize(target)) { cursor = cursor.resolve(part); if (Files.isSymbolicLink(cursor)) throw new IllegalArgumentException("SYMLINK_COMPONENT"); }
        if (!target.toRealPath().startsWith(root.toRealPath())) throw new IllegalArgumentException("REAL_TARGET_ESCAPES_WORKSPACE");
        return target;
    }
}
