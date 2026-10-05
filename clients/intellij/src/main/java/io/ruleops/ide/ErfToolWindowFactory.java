package io.ruleops.ide;

import com.google.gson.*;
import com.intellij.openapi.Disposable;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.icons.AllIcons;
import com.intellij.ui.OnePixelSplitter;
import com.intellij.ui.components.JBScrollPane;
import com.intellij.ui.components.JBTextArea;
import com.intellij.ui.components.JBTextField;
import com.intellij.ui.treeStructure.Tree;
import com.intellij.util.ui.JBUI;
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
import javax.accessibility.*;
import java.awt.event.ActionEvent;
import java.time.Instant;
import javax.swing.tree.*;
import java.awt.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

public final class ErfToolWindowFactory implements ToolWindowFactory {
    private static final Key<Panel> PANEL = Key.create("erf.toolWindow.panel");
    public enum Scene {
        SHOW("展示 Finding", "scan_findings", null), NAVIGATE("导航到位置", "scan_findings", null),
        EXPLAIN("解释 Finding", "scan_findings", "rf.explain"),
        SCAN("扫描结果工作台", "scan_findings", "rf.scan"),
        FEEDBACK("起草反馈", "feedback_exceptions", "rf.feedback.draft"),
        EXCEPTION("申请例外", "feedback_exceptions", "rf.exception.request"),
        SUBMIT_FEEDBACK("提交反馈", "feedback_exceptions", "rf.feedback.submit"),
        SUBMIT_EXCEPTION("提交例外", "feedback_exceptions", "rf.exception.submit"),
        STATUS("查询提交状态", "feedback_exceptions", "rf.submission.status"),
        REGISTRY_LIST("浏览规则库（元数据）", "registry", "rf.registry.list"),
        REGISTRY_SHOW("核对独立发布 pin", "registry", "rf.registry.show"),
        REGISTRY_SYNC("缓存签名规则包", "registry", "rf.registry.sync"),
        DOCTOR("环境诊断", "workspace", "rf.doctor"),
        CONFIG_VALIDATE("校验工作区配置", "workspace", "rf.config.validate"),
        CONFIG_SHOW("查看有效配置", "workspace", "rf.config.show"),
        STANDARD_VALIDATE("校验标准文件", "authoring_coverage", "rf.standard.validate"),
        RULE_VALIDATE("校验规则文件", "authoring_coverage", "rf.rule.validate");
        final String title, section, cli;
        final java.util.List<String> mappedFields;
        Scene(String title, String section, String cli) { this(title, section, cli, java.util.List.of()); }
        Scene(String title, String section, String cli, java.util.List<String> mappedFields) {
            this.title = title; this.section = section; this.cli = cli; this.mappedFields = mappedFields;
        }
        boolean registry() { return this == REGISTRY_LIST || this == REGISTRY_SHOW || this == REGISTRY_SYNC; }
        boolean server() { return this == SUBMIT_FEEDBACK || this == SUBMIT_EXCEPTION || this == STATUS; }
        /** 统一命令面场景：由同一 map params 驱动表单与 argv（ThinClientCore.buildMappedArgs）。 */
        boolean mapped() { return this == SCAN || this == DOCTOR || this == CONFIG_VALIDATE || this == CONFIG_SHOW
            || this == STANDARD_VALIDATE || this == RULE_VALIDATE; }
        boolean scan() { return this == SCAN; }
        boolean needsFinding() { return this == SHOW || this == NAVIGATE || this == EXPLAIN || this == FEEDBACK || this == EXCEPTION; }
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
    record RunRecord(Scene scene, String phase, String detail, Instant finishedAt) {
        @Override public String toString() { return scene.title + " · " + phase + " · " + finishedAt; }
    }
    static final class Panel extends JPanel implements Disposable {
        private final Project project;
        private final JTree tree;
        private final JTextArea context = new JBTextArea();
        private final JTextArea output = new JBTextArea();
        private final JLabel status = new JLabel() {
            @Override public AccessibleContext getAccessibleContext() {
                if (accessibleContext == null) accessibleContext = new AccessibleJLabel() {
                    @Override public AccessibleRole getAccessibleRole() { return AccessibleRole.STATUS_BAR; }
                };
                return accessibleContext;
            }
        };
        private final JPanel form = new JPanel();
        private final JTextField file = new JBTextField();
        private final JTextField actor = new JBTextField();
        private final JTextArea reason = new JBTextArea(3, 20);
        private final JComboBox<ThinClientCore.ScopeOption> scope = new JComboBox<>();
        private final JCheckBox actorConfirmed = new JCheckBox("我确认此 actor 是本次真实提交者");
        private final JCheckBox broaderConfirmed = new JCheckBox("我确认将申请范围扩大至所选 glob（非批准）");
        private final JComboBox<ErfArtifactIndex.Entry> artifacts = new JComboBox<>();
        private final JComboBox<String> selector = new JComboBox<>(new String[]{"requestId", "artifactDigest"});
        private final JTextField resultFile = new JBTextField();
        private final JTextField trustFile = new JBTextField();
        private final JComboBox<String> classification = new JComboBox<>(ThinClientCore.CLASSIFICATIONS.toArray(String[]::new));
        private final JButton execute = new JButton("执行所选场景");
        private final JButton cancel = new JButton("取消");
        private final JButton pick = new JButton("选择 JSON 文件");
        private final JButton refresh = new JButton("刷新上下文");
        private final DefaultMutableTreeNode runs = new DefaultMutableTreeNode("运行记录");
        private final ErfRegistry registry = new ErfRegistry();
        private final JComboBox<String> registryResource = new JComboBox<>(new String[]{"rules", "standards"});
        private final JComboBox<Integer> registryLimit = new JComboBox<>(new Integer[]{10, 50, 100, 256});
        private final JComboBox<ThinClientCore.RegistryPin> registryPins = new JComboBox<>();
        private final JTextField registryDescriptor = new JBTextField();
        private final JLabel registrySource = new JLabel("缺少独立发布来源");
        private final JButton nextPage = new JButton("下一页");
        private final java.util.LinkedHashMap<String, javax.swing.JComponent> mappedInputs = new java.util.LinkedHashMap<>();
        private final JTextArea scanProgress = new JBTextArea(3, 20);
        private boolean updatingRegistry;
        private Scene scene = Scene.SHOW;
        private RfInvoker.Job job;
        private boolean disposed;
        private long generation;
        private String endpoint = "";
        private String scopeFinding = "", lastScope;
        private boolean updatingScope;
        private String scopeEvidence = "尚未加载经过验证的范围";
        private JsonObject commandMap;

        Panel(Project project) {
            super(new BorderLayout(JBUI.scale(8), JBUI.scale(8))); this.project = project;
            setBorder(JBUI.Borders.empty(8));
            DefaultMutableTreeNode root = new DefaultMutableTreeNode("ERF");
            try (InputStream stream = ErfToolWindowFactory.class.getResourceAsStream("/ide-command-map.json")) {
                if (stream == null) throw new IllegalStateException("MISSING_IDE_COMMAND_MAP");
                JsonObject map = JsonParser.parseReader(new InputStreamReader(stream, StandardCharsets.UTF_8)).getAsJsonObject();
                commandMap = map;
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
            root.add(runs); tree = new Tree(root); tree.setRootVisible(false);
            tree.setCellRenderer(new DefaultTreeCellRenderer() {
                @Override public Component getTreeCellRendererComponent(JTree tree, Object value, boolean selected,
                        boolean expanded, boolean leaf, int row, boolean focus) {
                    putClientProperty("html.disable", Boolean.TRUE);
                    super.getTreeCellRendererComponent(tree, value, selected, expanded, leaf, row, focus);
                    Object item = ((DefaultMutableTreeNode) value).getUserObject();
                    setIcon(item instanceof Scene ? AllIcons.Actions.Execute : item instanceof RunRecord
                        ? AllIcons.Actions.Refresh : AllIcons.Nodes.Folder);
                    getAccessibleContext().setAccessibleName(item.toString());
                    return this;
                }
            });
            accessible(tree, "ERF 场景与运行记录");
            accessible(form, "场景参数");
            accessible(context, "当前工作区、连接与治理边界");
            accessible(output, "执行回执与输出日志");
            accessible(file, "当前选定的 FindingView 文件路径");
            accessible(scanProgress, "扫描实时进度");
            accessible(status, "运行状态");
            JPanel right = new JPanel(new BorderLayout(JBUI.scale(6), JBUI.scale(6)));
            context.setEditable(false); context.setLineWrap(true); context.setWrapStyleWord(true);
            right.add(context, BorderLayout.NORTH);
            form.setLayout(new BoxLayout(form, BoxLayout.Y_AXIS));
            output.setEditable(false); output.setLineWrap(true); output.setWrapStyleWord(true);
            JBScrollPane formScroll = new JBScrollPane(form);
            formScroll.setBorder(JBUI.Borders.empty());
            accessible(formScroll, "场景参数（可滚动）");
            formScroll.setHorizontalScrollBarPolicy(ScrollPaneConstants.HORIZONTAL_SCROLLBAR_NEVER);
            formScroll.setMinimumSize(new Dimension(0, 0));
            JBScrollPane outputScroll = new JBScrollPane(output);
            outputScroll.setMinimumSize(new Dimension(0, 0));
            accessible(outputScroll, "执行回执（可滚动）");
            OnePixelSplitter details = new OnePixelSplitter(true, .55f);
            details.setFirstComponent(formScroll); details.setSecondComponent(outputScroll);
            right.add(details, BorderLayout.CENTER);
            JPanel buttons = new JPanel(new GridLayout(0, 2, JBUI.scale(6), JBUI.scale(6)));
            JButton settings = new JButton("ERF 设置", AllIcons.General.Settings);
            execute.setIcon(AllIcons.Actions.Execute); cancel.setIcon(AllIcons.Actions.Suspend);
            refresh.setIcon(AllIcons.Actions.Refresh); pick.setIcon(AllIcons.Nodes.Folder);
            for (JButton button : List.of(execute, cancel, refresh, settings)) buttons.add(button);
            JPanel footer = new JPanel(new BorderLayout()); footer.add(status, BorderLayout.NORTH); footer.add(buttons, BorderLayout.SOUTH);
            right.add(footer, BorderLayout.SOUTH);
            OnePixelSplitter split = new OnePixelSplitter(true, .25f);
            split.setFirstComponent(new JBScrollPane(tree)); split.setSecondComponent(right); add(split);
            getInputMap(WHEN_ANCESTOR_OF_FOCUSED_COMPONENT).put(KeyStroke.getKeyStroke("ESCAPE"), "cancelErf");
            getActionMap().put("cancelErf", new AbstractAction() {
                @Override public void actionPerformed(ActionEvent event) { if (job != null) job.cancel(); }
            });
            file.getDocument().addDocumentListener(new javax.swing.event.DocumentListener() {
                public void insertUpdate(javax.swing.event.DocumentEvent e) { refresh(); }
                public void removeUpdate(javax.swing.event.DocumentEvent e) { refresh(); }
                public void changedUpdate(javax.swing.event.DocumentEvent e) { refresh(); }
            });
            file.setEditable(false);
            pick.addActionListener(e -> {
                var chosen = FileChooser.chooseFile(FileChooserDescriptorFactory.createSingleFileDescriptor("json"), project, null);
                if (chosen != null) { file.setText(chosen.getPath()); scopeFinding = ""; lastScope = null; clearScopes(); if (scene == Scene.EXCEPTION) loadScopes(); }
            });
            tree.addTreeSelectionListener(e -> {
                if (job != null) return;
                var node = (DefaultMutableTreeNode) tree.getLastSelectedPathComponent();
                if (node != null && node.getUserObject() instanceof Scene s) setScene(s);
                else if (node != null && node.getUserObject() instanceof RunRecord record)
                    setState("运行记录 · " + record.phase(), record.scene().title + "\n时间：" + record.finishedAt() + "\n" + record.detail());
                else if (node != null && node.getUserObject() instanceof Unavailable unavailable) {
                    scene = null; form.removeAll(); form.revalidate(); form.repaint();
                    setState("不可用", "尚未提供此命令的参数面板，不执行模板或自动准备安装资产。\n" + unavailable.template()); execute.setEnabled(false);
                }
            });
            execute.addActionListener(e -> execute()); cancel.addActionListener(e -> { if (job != null) job.cancel(); });
            refresh.addActionListener(e -> { if (scene != null && scene.registry()) registry.resetPage(); refresh(); if (scene == Scene.EXCEPTION && !file.getText().isBlank()) loadScopes(); });
            scope.addActionListener(e -> {
                if (updatingScope) return;
                broaderConfirmed.setSelected(false);
                var selected = (ThinClientCore.ScopeOption) scope.getSelectedItem();
                broaderConfirmed.setEnabled(selected != null && selected.broader());
                if (selected != null) lastScope = selected.value();
                if (scene == Scene.EXCEPTION && job == null) updateActions();
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
            // Swing labels otherwise interpret <html> in an external identity and may fetch images.
            var pinRenderer = new DefaultListCellRenderer();
            pinRenderer.putClientProperty("html.disable", Boolean.TRUE); registryPins.setRenderer(pinRenderer);
            registryLimit.setSelectedItem(50);
            registryResource.addActionListener(e -> {
                registry.resource((String) registryResource.getSelectedItem()); refreshPins();
                nextPage.setEnabled(false);
            });
            registryLimit.addActionListener(e -> { registry.resetPage(); nextPage.setEnabled(false); });
            registryPins.addActionListener(e -> {
                if (updatingRegistry) return;
                registry.select((ThinClientCore.RegistryPin) registryPins.getSelectedItem());
                registryDescriptor.setText(registry.descriptor());
            });
            registryDescriptor.getDocument().addDocumentListener(new javax.swing.event.DocumentListener() {
                public void insertUpdate(javax.swing.event.DocumentEvent e) { registry.descriptor(registryDescriptor.getText()); }
                public void removeUpdate(javax.swing.event.DocumentEvent e) { registry.descriptor(registryDescriptor.getText()); }
                public void changedUpdate(javax.swing.event.DocumentEvent e) { registry.descriptor(registryDescriptor.getText()); }
            });
            nextPage.addActionListener(e -> executeRegistry(true));
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
            mappedInputs.clear();
            form.add(new JLabel(next.title));
            if (next.mapped()) { mappedForm(next); }
            if (!next.server() && !next.registry() && !next.mapped()) { form.add(pick); field("FindingView 文件路径", file); }
            if (next.registry()) registryForm(next);
            if (next == Scene.FEEDBACK) field("分类", classification);
            if (next == Scene.FEEDBACK || next == Scene.EXCEPTION) {
                field("提交者", actor); form.add(actorConfirmed); field("理由（原文传 CLI；当前 CLI 不接受换行/控制符）", new JBScrollPane(reason));
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
            execute.setText(next.scan() ? "预览并扫描（结果不等于 PASS）" : next == Scene.REGISTRY_LIST ? "读取/刷新第一页" : next == Scene.REGISTRY_SHOW ? "核对独立发布 pin" : next == Scene.REGISTRY_SYNC ? "确认缓存签名包" : next == Scene.FEEDBACK || next == Scene.EXCEPTION ? "预览并保存草稿/申请" : "预览并执行");
            form.revalidate(); form.repaint(); refresh();
            if (next == Scene.EXCEPTION && !file.getText().isBlank()) loadScopes();
        }
        private static void accessible(JComponent component, String name) {
            component.getAccessibleContext().setAccessibleName(name);
            component.getAccessibleContext().setAccessibleDescription(name);
        }
        private void field(String text, JComponent input) {
            JComponent target = input instanceof JScrollPane scroll ? (JComponent) scroll.getViewport().getView() : input;
            JLabel label = new JLabel(text); label.setLabelFor(target); accessible(target, text);
            label.setBorder(JBUI.Borders.emptyTop(6)); form.add(label); form.add(input);
        }
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
            artifacts.setSelectedItem(null); clearScopes(); configureRegistry(); refreshPins();
            if (job != null) job.cancel();
            else { refreshArtifacts(null); refresh(); }
        }
        private void refresh() {
            if (job != null) return;
            ErfSettings.State c = project.getService(ErfSettings.class).snapshot();
            if (!endpoint.equals(c.serverEndpoint)) { endpoint = c.serverEndpoint; refreshArtifacts(null); }
            configureRegistry();
            context.setText("工作区：" + project.getBasePath() + "\nServer：" + (endpoint.isBlank() ? "未配置" : "已配置（由 CLI 校验连接）") + "\n提交 ≠ 批准 ≠ Gate PASS；不写入 SCM。");
            cancel.setEnabled(false);
            boolean ready = ready();
            updateActions();
            if (scene == null) setState("不可用", "尚未提供此场景参数面板。");
            else if (scene.needsFinding() && file.getText().isBlank()) setState("缺少 Finding", "请选择 FindingView JSON 文件后执行。");
            else if (scene == Scene.EXCEPTION && scope.getSelectedItem() == null) setState("范围未就绪", scopeEvidence + "；请选择 Finding 并刷新上下文，不回退整仓库。");
            else setState(ready ? "就绪" : "未配置", ready ? "参数将在执行前预览；保存后才可选择同一对象提交。" : "请配置工作区、rf 路径、正整数超时及所需 Server endpoint。");
        }
        private boolean ready() {
            var c = project.getService(ErfSettings.class).snapshot();
            return scene != null && project.getBasePath() != null && !c.rfPath.isBlank() && c.requestTimeoutMs > 0
                && (!scene.needsFinding() || !file.getText().isBlank())
                && (!scene.server() || !endpoint.isBlank())
                && (!scene.registry() || !c.connectionProfile.isBlank() || !endpoint.isBlank());
        }
        private void updateActions() {
            boolean ready = job == null && ready();
            execute.setEnabled(ready && (scene != Scene.EXCEPTION || scope.getSelectedItem() != null));
            nextPage.setEnabled(ready && scene == Scene.REGISTRY_LIST && registry.hasMore());
        }
        private void clearScopes() {
            updatingScope = true; scope.removeAllItems(); updatingScope = false;
            broaderConfirmed.setSelected(false); scopeEvidence = "尚无可靠 scope 选项";
        }
        private void setState(String phase, String detail) {
            String previous = status.getText(); status.setText(phase); output.setText(detail); output.setCaretPosition(0);
            status.getAccessibleContext().setAccessibleName("运行状态：" + phase);
            status.getAccessibleContext().firePropertyChange(AccessibleContext.ACCESSIBLE_VISIBLE_DATA_PROPERTY, previous, phase);
        }
        private static void enableTree(Component component, boolean enabled) {
            component.setEnabled(enabled);
            if (component instanceof Container container) for (Component child : container.getComponents()) enableTree(child, enabled);
        }
        private void busy(boolean value) {
            cancel.setEnabled(value); tree.setEnabled(!value); refresh.setEnabled(!value); enableTree(form, !value);
            if (value) { execute.setEnabled(false); nextPage.setEnabled(false); cancel.requestFocusInWindow(); }
            else updateActions();
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
                updateActions();
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
            if (scene.registry()) { executeRegistry(false); return; }
            if (scene.mapped()) { executeMapped(); return; }
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
                JTextArea previewText = new JBTextArea(preview, 18, 65); accessible(previewText, "实际 CLI 参数预览"); previewText.setEditable(false); previewText.setLineWrap(true); previewText.setWrapStyleWord(true);
                if (JOptionPane.showConfirmDialog(this, new JBScrollPane(previewText), "确认实际 CLI 参数", JOptionPane.OK_CANCEL_OPTION) != JOptionPane.OK_OPTION) return;
                if (submit) { entry.status = "UNKNOWN"; index().remember(entry); }
                run(request);
            } catch (IllegalArgumentException error) { setState("输入无效", error.getMessage() + "\n表单内容已保留。"); }
        }
        private void configureRegistry() {
            var c = project.getService(ErfSettings.class).snapshot();
            registry.configure(project.getBasePath() == null ? "" : project.getBasePath(), c.serverEndpoint, c.connectionProfile, c.requestTimeoutMs);
        }
        private void refreshPins() {
            updatingRegistry = true; registryPins.removeAllItems();
            for (var pin : registry.pins()) registryPins.addItem(pin);
            registryPins.setSelectedItem(null); updatingRegistry = false;
            registry.select(null); registryDescriptor.setText("");
            registrySource.setText(registry.source().isEmpty() ? "缺少独立发布来源；禁止从响应生成 pin" : registry.source());
        }
        private void registryForm(Scene next) {
            configureRegistry();
            var c = project.getService(ErfSettings.class).snapshot();
            form.add(new JLabel(c.connectionProfile.isBlank() ? "连接：显式 Server endpoint，由 CLI 校验" : "连接 profile FILE：" + c.connectionProfile + "（优先；失败不回退）"));
            form.add(new JLabel("FORMAT_ONLY / PIN_MATCH · UNKNOWN；元数据核对不是安装或可执行证明"));
            if (next == Scene.REGISTRY_SYNC) { registryResource.setSelectedItem("rules"); form.add(new JLabel("资源：rules")); }
            else field("资源类型", registryResource);
            JButton load = new JButton("选择独立发布记录 JSON（非信任根）");
            load.addActionListener(e -> loadReleasePins()); form.add(load); form.add(registrySource);
            if (next == Scene.REGISTRY_LIST) { field("每页条数", registryLimit); form.add(nextPage); }
            else field("独立身份 / 版本 / 摘要 / provenance（整条记录）", registryPins);
            if (next == Scene.REGISTRY_SYNC) fileField("外部签名 descriptor（含 cacheRoot；非 registry 响应）", registryDescriptor);
        }
        private void loadReleasePins() {
            if (job != null) return;
            var chosen = FileChooser.chooseFile(FileChooserDescriptorFactory.createSingleFileDescriptor("json"), project, null);
            if (chosen == null) return;
            long version = generation; Path source = Path.of(chosen.getPath());
            busy(true); setState("读取独立发布来源", source.toString());
            job = project.getService(RfInvoker.class).submit(project, indicator -> {
                var document = ErfWorkflow.object(Files.readString(WorkflowFiles.checkedPath(source, false)));
                ThinClientCore.projectRegistryPins(document); return document;
            }, document -> {
                job = null; if (disposed) return; busy(false);
                if (version != generation) { refresh(); return; }
                registry.loadPins(document, source); refreshPins(); refresh();
            }, error -> {
                job = null; if (disposed) return; busy(false);
                if (version != generation) { refresh(); return; }
                setState("来源无效", error.toString());
            }, () -> { job = null; if (!disposed) { busy(false); refresh(); setState("已取消", "未改变独立发布记录"); } });
        }
        /** 统一命令面表单：直接消费同一 map 的 params（kind→输入控件），不在此定义 wire schema。 */
        private void mappedForm(Scene next) {
            JsonObject command = findCommand(next.cli);
            if (command == null || !command.has("params")) { setState("不可用", "map 缺少 " + next.cli + " 的 params"); return; }
            String basePath = project.getBasePath() == null ? "" : project.getBasePath();
            for (var element : command.getAsJsonArray("params")) {
                JsonObject p = element.getAsJsonObject();
                String name = p.get("name").getAsString();
                String label = name.equals("deadlineMs") ? "deadline 毫秒（缺省 600 秒）"
                    : p.has("label") ? p.get("label").getAsString() : name;
                String kind = p.get("kind").getAsString();
                boolean required = p.has("required") && p.get("required").getAsBoolean();
                if ("bool".equals(kind)) {
                    JCheckBox box = new JCheckBox(required ? label + "（必选）" : label);
                    if (required) box.setSelected(true);
                    mappedInputs.put(name, box); form.add(box);
                    continue;
                }
                if (p.has("values")) {
                    JComboBox<String> combo = new JComboBox<>();
                    for (var v : p.getAsJsonArray("values")) combo.addItem(v.getAsString());
                    if (!required) combo.insertItemAt("", 0);
                    combo.setSelectedItem(required ? combo.getItemAt(0) : "");
                    if (next.scan() && name.equals("progress")) combo.setSelectedItem("text");
                    if (next.scan() && name.equals("format")) combo.setSelectedItem("json");
                    mappedInputs.put(name, combo); field(label + (required ? "（必选）" : ""), combo);
                    continue;
                }
                JTextField input = new JBTextField();
                // repoRoot 来自当前工作区选择，不由用户键入。
                if ("repoRoot".equals(name)) { input.setText(basePath); input.setEditable(false); }
                if ("route".equals(name)) input.setText("rule-id");
                mappedInputs.put(name, input);
                field(label + (required ? "（必选）" : "") + ("number".equals(kind) ? "（数字）" : ""), input);
            }
            if (next.scan()) {
                scanProgress.setEditable(false); scanProgress.setLineWrap(true);
                form.add(new JLabel("扫描进度（stderr progress 行；长任务不按 5 秒截断）"));
                form.add(new JBScrollPane(scanProgress));
            }
        }
        private JsonObject findCommand(String commandId) {
            if (commandMap == null) return null;
            for (var element : commandMap.getAsJsonArray("commands")) {
                JsonObject command = element.getAsJsonObject();
                if (commandId.equals(command.get("commandId").getAsString())) return command;
            }
            return null;
        }
        private void executeMapped() {
            if (job != null || scene == null || !scene.mapped()) return;
            try {
                JsonObject command = findCommand(scene.cli);
                if (command == null) throw new IllegalArgumentException("COMMAND_MAP_MISSING:" + scene.cli);
                java.util.LinkedHashMap<String, ThinClientCore.Param> params = new java.util.LinkedHashMap<>();
                java.util.Map<String, String> values = new java.util.LinkedHashMap<>();
                for (var element : command.getAsJsonArray("params")) {
                    JsonObject p = element.getAsJsonObject();
                    String name = p.get("name").getAsString();
                    params.put(name, new ThinClientCore.Param(name, p.get("kind").getAsString(),
                        p.has("flag") ? p.get("flag").getAsString() : null,
                        p.has("required") && p.get("required").getAsBoolean(),
                        p.has("values") ? toList(p.getAsJsonArray("values")) : List.of(),
                        p.has("hexDigest") && p.get("hexDigest").getAsBoolean(),
                        p.has("repeatable") && p.get("repeatable").getAsBoolean(),
                        p.has("identity") && p.get("identity").getAsBoolean(),
                        p.has("routeSelector") && p.get("routeSelector").getAsBoolean()));
                    if (mappedInputs.get(name) instanceof JCheckBox box) { if (box.isSelected()) values.put(name, "true"); }
                    else if (mappedInputs.get(name) instanceof JComboBox<?> combo) {
                        Object selected = combo.getSelectedItem();
                        if (selected != null && !String.valueOf(selected).isEmpty()) values.put(name, String.valueOf(selected));
                    } else if (mappedInputs.get(name) instanceof JTextField input) {
                        String text = input.getText().trim();
                        if (!text.isEmpty()) values.put(name, text);
                    }
                }
                String template = command.get("commandTemplate").getAsString();
                List<String> argv = scene.scan()
                    ? ThinClientCore.buildScanArgs(template, params.values().stream().toList(), values)
                    : ThinClientCore.buildMappedArgs(template, List.copyOf(params.values()), values);
                String preview = "rf 参数数组（无 shell）：\n" + new GsonBuilder().setPrettyPrinting().create().toJson(argv)
                    + (scene.scan() ? "\nexit=0 不代表 Gate PASS；结果以结构化 status/gate/authority 为准。" : "");
                JTextArea text = new JBTextArea(preview, 16, 65); accessible(text, "实际 CLI 参数预览"); text.setEditable(false); text.setLineWrap(true); text.setWrapStyleWord(true);
                if (JOptionPane.showConfirmDialog(this, new JBScrollPane(text), "确认实际 CLI 参数", JOptionPane.OK_CANCEL_OPTION) != JOptionPane.OK_OPTION) return;
                long version = generation;
                var c = project.getService(ErfSettings.class).snapshot();
                var invoker = project.getService(RfInvoker.class);
                String base = project.getBasePath();
                busy(true); scanProgress.setText(""); setState("运行中", scene.title);
                job = invoker.submit(project, indicator -> invoker.run(c.rfPath, argv, base, c.requestTimeoutMs, indicator,
                    line -> ApplicationManager.getApplication().invokeLater(() -> {
                        if (disposed || version != generation || job == null) return;
                        if (scene.scan()) scanProgress.append(line + "\n");
                        else output.append("\n" + line);
                    })), result -> {
                    job = null; if (disposed) return; busy(false);
                    if (version != generation) { refresh(); return; }
                    if (scene.scan()) {
                        scanProgress.setText(result.stderr() == null ? "" : result.stderr());
                        try {
                            var parsed = ErfWorkflow.object(result.stdout());
                            // exit=0 也按结构化 status 渲染；绝不显示为简单通过。
                            finish("扫描结果 " + parsed.get("status") + "（非 PASS 判定）",
                                ThinClientCore.renderScanReport(parsed, result.exitCode(), result.stderr()));
                        } catch (RuntimeException parseError) {
                            finish("扫描输出无法解析", "exit=" + result.exitCode() + "\n" + result.stdout() + "\n" + result.stderr());
                        }
                        return;
                    }
                    finish(result.exitCode() == 0 ? "完成（无批准权威）" : "失败 exit=" + result.exitCode(),
                        result.exitCode() == 0 ? result.stdout() : (result.stderr().isBlank() ? result.stdout() : result.stderr()));
                }, error -> {
                    job = null; if (disposed) return; busy(false);
                    if (version != generation) { refresh(); return; }
                    finish(error instanceof RfInvoker.UnavailableException ? "不可用" : "失败", error.toString() + "\n表单保留。");
                }, () -> {
                    job = null; if (disposed) return; busy(false);
                    if (version != generation) { refresh(); return; }
                    finish("已取消", "表单保留；扫描如有缓存写入，以再次扫描结果为准。");
                });
            } catch (IllegalArgumentException error) { setState("输入无效", error.getMessage() + "\n表单内容已保留。"); }
        }
        private static List<String> toList(com.google.gson.JsonArray array) {
            List<String> out = new ArrayList<>();
            for (var element : array) out.add(element.getAsString());
            return out;
        }
        private void executeRegistry(boolean next) {
            if (job != null || scene == null || !scene.registry()) return;
            try {
                configureRegistry();
                String action = scene == Scene.REGISTRY_LIST ? "list" : scene == Scene.REGISTRY_SHOW ? "show" : "sync";
                var request = registry.prepare(action, next, (Integer) registryLimit.getSelectedItem());
                var c = project.getService(ErfSettings.class).snapshot(); String base = project.getBasePath(); long version = generation;
                String preview = new GsonBuilder().setPrettyPrinting().create().toJson(request.args()) + "\n独立来源：" + (request.pin() == null ? "无（仅浏览）" : request.pin().provenance())
                    + "\n只调用 CLI。PIN_MATCH 不是安装；sync 仅按外部 descriptor 缓存，后续扫描仍须验证。";
                JTextArea text = new JBTextArea(preview, 16, 65); accessible(text, "实际 CLI 参数预览"); text.setEditable(false); text.setLineWrap(true); text.setWrapStyleWord(true);
                if (JOptionPane.showConfirmDialog(this, new JBScrollPane(text), "确认实际 CLI 参数", JOptionPane.OK_CANCEL_OPTION) != JOptionPane.OK_OPTION) return;
                if (version != generation || disposed) return;
                var invoker = project.getService(RfInvoker.class); busy(true); setState("运行中", "registry " + action);
                job = invoker.submit(project, indicator -> {
                    var result = invoker.run(c.rfPath, request.args(), base, c.requestTimeoutMs, indicator);
                    if (result.exitCode() != 0) throw new IllegalArgumentException("UNKNOWN · 未安装 · exit=" + result.exitCode() + "\n" + (result.stderr().isBlank() ? result.stdout() : result.stderr()));
                    return ErfWorkflow.object(result.stdout());
                }, receipt -> {
                    job = null; if (disposed) return; busy(false);
                    if (version != generation) { refresh(); return; }
                    try { String detail = registry.accept(request, receipt); refresh(); finish("完成（无批准权威）", detail); }
                    catch (IllegalArgumentException error) { finish("UNKNOWN / 失败 / 未安装", error.getMessage()); }
                }, error -> {
                    job = null; if (disposed) return; busy(false);
                    if (version != generation) { refresh(); return; }
                    finish("UNKNOWN / 失败 / 未安装", error.toString());
                }, () -> {
                    job = null; if (disposed) return; busy(false);
                    if (version != generation) { refresh(); return; }
                    finish("已取消 / UNKNOWN", "缓存副作用未知；未安装。不会自动重试或改连服务。");
                });
            } catch (IllegalArgumentException error) { setState("输入无效 / UNKNOWN", error.getMessage()); }
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
            runs.insert(new DefaultMutableTreeNode(new RunRecord(scene, phase, detail, Instant.now())), 0);
            if (runs.getChildCount() > 20) runs.remove(20);
            ((DefaultTreeModel) tree.getModel()).reload(runs);
        }
        @Override public void dispose() { disposed = true; generation++; if (job != null) job.cancel(); project.putUserData(PANEL, null); }
    }
}
