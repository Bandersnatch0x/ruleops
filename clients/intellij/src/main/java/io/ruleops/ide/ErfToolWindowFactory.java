package io.ruleops.ide;

import com.google.gson.*;
import com.intellij.openapi.Disposable;
import com.intellij.openapi.fileChooser.FileChooser;
import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory;
import com.intellij.openapi.fileChooser.FileChooserFactory;
import com.intellij.openapi.fileChooser.FileSaverDescriptor;
import com.intellij.openapi.fileEditor.OpenFileDescriptor;
import com.intellij.openapi.options.ShowSettingsUtil;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.util.Key;
import com.intellij.openapi.vfs.LocalFileSystem;
import com.intellij.openapi.wm.ToolWindow;
import com.intellij.openapi.wm.ToolWindowFactory;
import com.intellij.openapi.wm.ToolWindowManager;
import com.intellij.ui.content.Content;
import com.intellij.ui.content.ContentFactory;
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
        EXCEPTION("申请例外", "feedback_exceptions", "rf.exception.request"),
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
        if (panel != null) panel.settingsChanged();
    }
    private record Unavailable(String template) { @Override public String toString() { return template + " · 不可用"; } }
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
        private final JComboBox<ThinClientCore.ScopeOption> scope = new JComboBox<>();
        private final JCheckBox actorConfirmed = new JCheckBox("我确认此 actor 是本次真实提交者");
        private final JCheckBox broaderConfirmed = new JCheckBox("我确认将申请范围扩大至所选 glob（非批准）");
        private final JComboBox<ErfArtifactIndex.Entry> artifacts = new JComboBox<>();
        private final JComboBox<String> selector = new JComboBox<>(new String[]{"requestId", "artifactDigest"});
        private final JTextField resultFile = new JTextField();
        private final JTextField trustFile = new JTextField();
        private final JComboBox<String> classification = new JComboBox<>(ThinClientCore.CLASSIFICATIONS.toArray(String[]::new));
        private final JButton execute = new JButton("执行所选场景");
        private final JButton cancel = new JButton("取消");
        private final JButton pick = new JButton("选择 JSON 文件");
        private final JButton refresh = new JButton("刷新上下文");
        private final DefaultMutableTreeNode runs = new DefaultMutableTreeNode("运行记录");
        private Scene scene = Scene.SHOW;
        private RfInvoker.Job job;
        private boolean disposed;
        private long generation;
        private String endpoint = "";
        private String scopeFinding = "", lastScope;
        private boolean updatingScope;
        private String scopeEvidence = "尚未加载经过验证的范围";

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
                if (chosen != null) { file.setText(chosen.getPath()); scopeFinding = ""; lastScope = null; clearScopes(); if (scene == Scene.EXCEPTION) loadScopes(); }
            });
            tree.addTreeSelectionListener(e -> {
                if (job != null) return;
                var node = (DefaultMutableTreeNode) tree.getLastSelectedPathComponent();
                if (node != null && node.getUserObject() instanceof Scene s) setScene(s);
                else if (node != null && node.getUserObject() instanceof Unavailable unavailable) {
                    scene = null; form.removeAll(); form.revalidate(); form.repaint();
                    setState("不可用", "尚未提供此命令的参数面板，不执行模板或自动准备安装资产。\n" + unavailable.template()); execute.setEnabled(false);
                }
            });
            execute.addActionListener(e -> execute()); cancel.addActionListener(e -> { if (job != null) job.cancel(); });
            refresh.addActionListener(e -> { refresh(); if (scene == Scene.EXCEPTION && !file.getText().isBlank()) loadScopes(); });
            scope.addActionListener(e -> {
                if (updatingScope) return;
                broaderConfirmed.setSelected(false);
                var selected = (ThinClientCore.ScopeOption) scope.getSelectedItem();
                broaderConfirmed.setEnabled(selected != null && selected.broader());
                if (selected != null) lastScope = selected.value();
                if (scene == Scene.EXCEPTION && job == null) execute.setEnabled(selected != null);
                if (selected != null) output.setText("将传入 scope：" + selected.value() + "\n来源：" + selected.source()
                    + "\n当前文件匹配：是（执行前复验）\n" + scopeEvidence);
            });
            actor.getDocument().addDocumentListener(new javax.swing.event.DocumentListener() {
                public void insertUpdate(javax.swing.event.DocumentEvent e) { actorConfirmed.setSelected(false); }
                public void removeUpdate(javax.swing.event.DocumentEvent e) { actorConfirmed.setSelected(false); }
                public void changedUpdate(javax.swing.event.DocumentEvent e) { actorConfirmed.setSelected(false); }
            });
            reason.setLineWrap(true); reason.setWrapStyleWord(true);
            settings.addActionListener(e -> ShowSettingsUtil.getInstance().showSettingsDialog(project, ErfConfigurable.class));
            select(Scene.SHOW);
        }
        void select(Scene next) {
            if (job != null) return;
            var nodes = ((DefaultMutableTreeNode) tree.getModel().getRoot()).depthFirstEnumeration();
            while (nodes.hasMoreElements()) {
                var node = (DefaultMutableTreeNode) nodes.nextElement();
                if (node.getUserObject() == next) {
                    TreePath path = new TreePath(node.getPath());
                    boolean alreadySelected = path.equals(tree.getSelectionPath());
                    tree.setSelectionPath(path); tree.scrollPathToVisible(path);
                    if (alreadySelected) setScene(next);
                    return;
                }
            }
            throw new IllegalArgumentException("SCENE_NOT_REGISTERED");
        }
        private void setScene(Scene next) {
            var previousArtifact = (ErfArtifactIndex.Entry) artifacts.getSelectedItem();
            scene = next; generation++; form.removeAll();
            form.add(new JLabel(next.title));
            if (!next.server()) { form.add(pick); form.add(file); }
            if (next == Scene.FEEDBACK) field("分类", classification);
            if (next == Scene.FEEDBACK || next == Scene.EXCEPTION) {
                field("提交者", actor); form.add(actorConfirmed); field("理由（原文传 CLI；当前 CLI 不接受换行/控制符）", new JScrollPane(reason));
            }
            if (next == Scene.EXCEPTION) { field("scope（文件最窄默认；目录/module 须确认）", scope); form.add(broaderConfirmed); }
            if (next == Scene.EXPLAIN) {
                fileField("result（可选，须与 authority-trust 成对）", resultFile);
                fileField("authority-trust（独立可信文件）", trustFile);
                JButton clear = new JButton("清除解释辅助文件");
                clear.addActionListener(e -> { resultFile.setText(""); trustFile.setText(""); }); form.add(clear);
            }
            if (next.server()) { field("本工作区已保存请求 / 回执（无需手选 JSON）", artifacts); refreshArtifacts(previousArtifact); }
            if (next == Scene.STATUS) field("查询身份（从所选对象读取）", selector);
            execute.setText(next == Scene.FEEDBACK || next == Scene.EXCEPTION ? "预览并保存草稿/申请" : "预览并执行");
            form.revalidate(); form.repaint(); refresh();
            if (next == Scene.EXCEPTION && !file.getText().isBlank()) loadScopes();
        }
        private void field(String text, JComponent input) { JLabel label = new JLabel(text); label.setLabelFor(input); form.add(label); form.add(input); }
        private void fileField(String label, JTextField input) {
            input.setEditable(false); field(label, input);
            JButton choose = new JButton("选择 " + label.split("（")[0]);
            choose.addActionListener(e -> {
                var chosen = FileChooser.chooseFile(FileChooserDescriptorFactory.createSingleFileDescriptor("json"), project, null);
                if (chosen != null) input.setText(chosen.getPath());
            }); form.add(choose);
        }
        private ErfArtifactIndex index() { return project.getService(ErfArtifactIndex.class); }
        private void refreshArtifacts(ErfArtifactIndex.Entry preferred) {
            artifacts.removeAllItems();
            String base = project.getBasePath();
            for (var entry : index().entries()) {
                if (!entry.workspace.equals(base) || (!entry.endpoint.isEmpty() && !entry.endpoint.equals(endpoint))) continue;
                if (scene == Scene.SUBMIT_FEEDBACK && !entry.kind.equals("FeedbackDraft")) continue;
                if (scene == Scene.SUBMIT_EXCEPTION && !entry.kind.equals("ExceptionRequest")) continue;
                artifacts.addItem(entry);
            }
            ErfArtifactIndex.Entry selected = null;
            if (preferred != null) for (int i = 0; i < artifacts.getItemCount(); i++) {
                var candidate = artifacts.getItemAt(i);
                if (candidate.path.equals(preferred.path) && candidate.endpoint.equals(preferred.endpoint)
                    && candidate.requestId.equals(preferred.requestId) && candidate.artifactDigest.equals(preferred.artifactDigest)) selected = candidate;
            }
            artifacts.setSelectedItem(selected);
        }
        private void settingsChanged() {
            generation++; endpoint = project.getService(ErfSettings.class).snapshot().serverEndpoint;
            artifacts.setSelectedItem(null); clearScopes();
            if (job != null) job.cancel();
            else { refreshArtifacts(null); refresh(); }
        }
        private void refresh() {
            if (job != null) return;
            ErfSettings.State c = project.getService(ErfSettings.class).snapshot();
            if (!endpoint.equals(c.serverEndpoint)) { endpoint = c.serverEndpoint; refreshArtifacts(null); }
            context.setText("工作区：" + project.getBasePath() + "\nServer：" + (endpoint.isBlank() ? "未配置" : "已配置（由 CLI 校验连接）") + "\n提交 ≠ 批准 ≠ Gate PASS；不写入 SCM。");
            cancel.setEnabled(false);
            boolean ready = scene != null && project.getBasePath() != null && !c.rfPath.isBlank() && c.requestTimeoutMs > 0 && (!scene.server() || !endpoint.isBlank());
            execute.setEnabled(ready && (scene != Scene.EXCEPTION || scope.getSelectedItem() != null));
            if (scene == null) setState("不可用", "尚未提供此场景参数面板。");
            else if (scene == Scene.EXCEPTION && scope.getSelectedItem() == null) setState("范围未就绪", scopeEvidence + "；请选择 Finding 并刷新上下文，不回退整仓库。");
            else setState(ready ? "就绪" : "未配置", ready ? "参数将在执行前预览；保存后才可选择同一对象提交。" : "请配置工作区、rf 路径、正整数超时及所需 Server endpoint。");
        }
        private void clearScopes() {
            updatingScope = true; scope.removeAllItems(); updatingScope = false;
            broaderConfirmed.setSelected(false); scopeEvidence = "尚无可靠 scope 选项";
        }
        private void setState(String phase, String detail) { status.setText(phase); output.setText(detail); }
        private static void enableTree(Component component, boolean enabled) {
            component.setEnabled(enabled);
            if (component instanceof Container container) for (Component child : container.getComponents()) enableTree(child, enabled);
        }
        private void busy(boolean value) {
            execute.setEnabled(!value); cancel.setEnabled(value); tree.setEnabled(!value); refresh.setEnabled(!value); enableTree(form, !value);
        }
        private void loadScopes() {
            if (job != null || disposed) return;
            clearScopes(); busy(true); setState("读取范围", "验证 Finding 文件与配置路径（只读 config show）。");
            long version = generation;
            var c = project.getService(ErfSettings.class).snapshot(); var invoker = project.getService(RfInvoker.class);
            String input = file.getText(), base = project.getBasePath();
            if (base == null || c.rfPath.isBlank()) { busy(false); refresh(); return; }
            job = invoker.submit(project, indicator -> ErfWorkflow.scopes(invoker, indicator, c, base, input), result -> {
                job = null; if (disposed) return; busy(false);
                if (version != generation) { refresh(); return; }
                scopeEvidence = result.rejected().isEmpty() ? "来自已验证 Finding、有效配置和工作区文件系统" : "已过滤不可信范围：" + result.rejected();
                var restored = ThinClientCore.restoreScopeSelection(result.options(), lastScope, !input.equals(scopeFinding));
                updatingScope = true;
                for (var option : result.options()) scope.addItem(option);
                scope.setSelectedItem(restored); updatingScope = false; scopeFinding = input;
                if (restored != null) lastScope = restored.value();
                broaderConfirmed.setSelected(false); broaderConfirmed.setEnabled(restored != null && restored.broader());
                execute.setEnabled(restored != null);
                if (restored == null) scopeEvidence = "原选择已失效或无可靠范围；请明确重新选择，不自动替换。" + scopeEvidence;
                setState(restored != null ? "范围已验证" : "范围不可用", scopeEvidence);
            }, error -> {
                job = null; if (disposed) return; busy(false);
                if (version != generation) { refresh(); return; }
                execute.setEnabled(false); scopeEvidence = error.toString(); setState("范围不可用", scopeEvidence);
            }, () -> {
                job = null; if (disposed) return; busy(false);
                if (version != generation) { refresh(); return; }
                execute.setEnabled(false); setState("已取消", "范围加载已取消；未授权任何范围。");
            });
        }
        private record Destination(String path, boolean overwriteConfirmed) {}
        private Destination destination(String label, String defaultName) {
            var saver = FileChooserFactory.getInstance().createSaveFileDialog(new FileSaverDescriptor(label, "选择明确的 JSON 输出位置", "json"), project);
            var base = project.getBasePath() == null ? null : LocalFileSystem.getInstance().findFileByPath(project.getBasePath());
            var selected = saver.save(base, defaultName);
            if (selected == null) return null;
            Path target = selected.getFile().toPath();
            boolean exists = Files.exists(target, LinkOption.NOFOLLOW_LINKS);
            if (exists && JOptionPane.showConfirmDialog(this, "明确覆盖文件？\n" + target, "确认覆盖", JOptionPane.OK_CANCEL_OPTION) != JOptionPane.OK_OPTION) return null;
            return new Destination(target.toString(), exists);
        }
        private void execute() {
            if (job != null || scene == null) return;
            refresh(); if (!execute.isEnabled()) return;
            try {
                Scene selected = scene; var c = project.getService(ErfSettings.class).snapshot();
                String base = project.getBasePath();
                boolean draft = selected == Scene.FEEDBACK || selected == Scene.EXCEPTION;
                boolean submit = selected == Scene.SUBMIT_FEEDBACK || selected == Scene.SUBMIT_EXCEPTION;
                if (draft && !actorConfirmed.isSelected()) throw new IllegalArgumentException("请明确确认真实 actor");
                var scopeOption = (ThinClientCore.ScopeOption) scope.getSelectedItem();
                String scopeValue = scopeOption == null ? "" : scopeOption.value();
                if (selected == Scene.EXCEPTION && scopeOption.broader() && !broaderConfirmed.isSelected())
                    throw new IllegalArgumentException("请确认扩大 scope：" + scopeValue);
                ErfArtifactIndex.Entry entry = selected.server() ? (ErfArtifactIndex.Entry) artifacts.getSelectedItem() : null;
                if (selected.server()) {
                    if (entry == null) throw new IllegalArgumentException("请选择本工作区已保存对象，不手抄摘要");
                    entry = ErfWorkflow.copy(entry); entry.endpoint = ErfWorkflow.checkedEndpoint(c.serverEndpoint);
                }
                // Validate fields before opening the save dialog; no side effects until the final preview is approved.
                var request = new ErfWorkflow.Request(selected, c, base, file.getText(), actor.getText(), reason.getText(),
                    String.valueOf(classification.getSelectedItem()), scopeValue, broaderConfirmed.isSelected(),
                    resultFile.getText(), trustFile.getText(), entry, selector.getSelectedIndex() == 1, "", false);
                ErfWorkflow.args(request);
                Destination dest = draft || submit ? destination(submit ? "保存提交回执" : "保存草稿/申请", submit ? "submission-receipt.json" : "request.json") : new Destination("", false);
                if (dest == null) { setState("已取消", "没有保存或提交，表单保留。"); return; }
                request = new ErfWorkflow.Request(selected, c, base, request.findingPath(), request.actor(), request.reason(), request.classification(),
                    scopeValue, request.broaderConfirmed(), request.result(), request.authorityTrust(), entry, request.byDigest(), dest.path(), dest.overwriteConfirmed());
                List<String> argv = ErfWorkflow.args(request);
                String preview = "rf：" + c.rfPath + "\n参数数组（无 shell）：\n" + new GsonBuilder().setPrettyPrinting().create().toJson(argv);
                if (entry != null) preview += "\nendpoint：" + entry.endpoint + "\nrequestId：" + entry.requestId + "\nartifactDigest：" + entry.artifactDigest
                    + "\n数据范围：digest-only 请求字段、actor、reason、classification/scope，不传源码。\n仅接收，非批准；未知结果只查状态、不自动重提。";
                JTextArea previewText = new JTextArea(preview, 18, 65); previewText.setEditable(false); previewText.setLineWrap(true); previewText.setWrapStyleWord(true);
                if (JOptionPane.showConfirmDialog(this, new JScrollPane(previewText), "确认实际 CLI 参数", JOptionPane.OK_CANCEL_OPTION) != JOptionPane.OK_OPTION) return;
                if (submit) { entry.status = "UNKNOWN"; index().remember(entry); }
                run(request);
            } catch (IllegalArgumentException error) { setState("输入无效", error.getMessage() + "\n表单内容已保留。"); }
        }
        private void run(ErfWorkflow.Request request) {
            long version = generation;
            var invoker = project.getService(RfInvoker.class);
            busy(true); setState("运行中", request.scene().title);
            job = invoker.submit(project, indicator -> ErfWorkflow.perform(invoker, indicator, request), result -> {
                job = null; if (disposed) return; busy(false);
                if (version != generation) { refreshArtifacts(null); refresh(); return; }
                if (result.target() != null) {
                    var target = LocalFileSystem.getInstance().refreshAndFindFileByNioFile(result.target());
                    if (target == null) { finish("失败", "目标已消失"); return; }
                    new OpenFileDescriptor(project, target, result.line(), result.column()).navigate(true);
                }
                if (result.artifact() != null) { index().remember(result.artifact()); refreshArtifacts(result.artifact()); }
                String phase = result.artifact() != null ? result.artifact().status : "完成（无批准权威）";
                finish(phase, result.detail());
                if (request.scene() == Scene.FEEDBACK || request.scene() == Scene.EXCEPTION) {
                    JButton submit = new JButton("提交刚保存的同一对象");
                    submit.addActionListener(e -> {
                        if (job != null) return;
                        select(request.scene() == Scene.FEEDBACK ? Scene.SUBMIT_FEEDBACK : Scene.SUBMIT_EXCEPTION);
                        artifacts.setSelectedItem(result.artifact());
                    }); form.add(submit); form.revalidate();
                }
            }, error -> {
                job = null; if (disposed) return; busy(false);
                if (version != generation) { refreshArtifacts(null); refresh(); return; }
                finish(error instanceof RfInvoker.UnavailableException ? "不可用" : request.scene().server() ? "UNKNOWN / 失败" : "失败",
                    error + "\n表单保留。远端写入结果不明时，请选择该对象查询状态，切勿自动重复提交。");
            }, () -> {
                job = null; if (disposed) return; busy(false);
                if (version != generation) { refreshArtifacts(null); refresh(); return; }
                finish("已取消", "表单保留；远端可能已接收。取消不是回滚，请查询状态。");
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
}
