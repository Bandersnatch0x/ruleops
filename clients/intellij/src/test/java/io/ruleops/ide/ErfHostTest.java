package io.ruleops.ide;

import com.intellij.openapi.application.ApplicationManager;
import com.intellij.testFramework.LightPlatformTestCase;
import com.intellij.util.ui.UIUtil;
import javax.xml.parsers.DocumentBuilderFactory;
import java.nio.file.Path;
import java.nio.file.Files;
import java.io.UncheckedIOException;
import java.time.Instant;
import java.awt.Component;
import java.awt.Container;
import java.util.ArrayList;
import java.util.function.Predicate;
import javax.accessibility.AccessibleContext;
import javax.accessibility.AccessibleRole;
import javax.swing.*;
import javax.swing.tree.DefaultMutableTreeNode;
import javax.swing.tree.TreePath;
import com.intellij.ui.components.JBScrollPane;
import com.intellij.ui.components.JBTextArea;
import com.intellij.ui.components.JBTextField;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

/** Requires the real IntelliJ SDK/test application. Not part of CoreSelfTest. */
public final class ErfHostTest extends LightPlatformTestCase {
    public void testToolWindowAndSettingsResourcesAndNativePanel() throws Exception {
        try (var xml = ErfToolWindowFactory.class.getResourceAsStream("/META-INF/plugin.xml")) {
            assertNotNull(xml);
            var document = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(xml);
            var window = document.getElementsByTagName("toolWindow").item(0).getAttributes();
            assertEquals("ERF", window.getNamedItem("id").getNodeValue());
            assertEquals(ErfToolWindowFactory.class.getName(), window.getNamedItem("factoryClass").getNodeValue());
            assertEquals(1, document.getElementsByTagName("projectConfigurable").getLength());
        }
        var panel = new ErfToolWindowFactory.Panel(getProject());
        try {
            assertTrue(panel.getComponentCount() > 0);
            for (var scene : ErfToolWindowFactory.Scene.values()) panel.select(scene);
            assertEquals(17, ErfToolWindowFactory.Scene.values().length);
            panel.select(ErfToolWindowFactory.Scene.REGISTRY_SHOW);
            assertTrue(hasPlainTextRegistryRenderer(panel));
        } finally { panel.dispose(); }
    }

    public void testConnectionProfileIsOnlyAnIndependentFileReference() {
        var settings = getProject().getService(ErfSettings.class);
        var previous = settings.snapshot();
        try {
            var state = new ErfSettings.State(); state.connectionProfile = "/release/connection profile.json";
            state.serverEndpoint = "http://127.0.0.1:9/v1"; settings.loadState(state);
            assertEquals(state.connectionProfile, settings.snapshot().connectionProfile);
            var configurable = new ErfConfigurable(getProject()); configurable.createComponent();
            assertFalse(configurable.isModified());
        } finally { settings.loadState(previous); }
    }

    public void testProcessWorkIsBackgroundAndCompletionIsEdt() throws Exception {
        AtomicBoolean complete = new AtomicBoolean();
        AtomicReference<Exception> error = new AtomicReference<>();
        var invoker = getProject().getService(RfInvoker.class);
        invoker.submit(getProject(), indicator -> {
            assertFalse(ApplicationManager.getApplication().isDispatchThread());
            String java = Path.of(System.getProperty("java.home"), "bin", System.getProperty("os.name").startsWith("Windows") ? "java.exe" : "java").toString();
            return invoker.run(java, List.of("-version"), System.getProperty("java.io.tmpdir"), 3000, indicator);
        }, result -> {
            assertTrue(ApplicationManager.getApplication().isDispatchThread());
            assertEquals(0, result.exitCode()); complete.set(true);
        }, ex -> { error.set(ex); complete.set(true); }, () -> fail("unexpected cancellation"));
        await(complete);
        if (error.get() != null) throw error.get();
    }

    public void testCancellationUsesTaskOnCancelOnEdt() throws Exception {
        AtomicBoolean entered = new AtomicBoolean();
        AtomicBoolean cancelled = new AtomicBoolean();
        var job = getProject().getService(RfInvoker.class).submit(getProject(), indicator -> {
            entered.set(true);
            while (true) { indicator.checkCanceled(); Thread.sleep(10); }
        }, ignored -> fail("cancelled task succeeded"), ex -> fail(ex.toString()), () -> {
            assertTrue(ApplicationManager.getApplication().isDispatchThread()); cancelled.set(true);
        });
        try { await(entered); job.cancel(); await(cancelled); }
        finally { job.cancel(); }
    }

    public void testLocalTimeoutPolicyKeepsNetworkBudgetsAndHonorsScanDeadline() {
        for (var args : List.of(List.of("scan"), List.of("doctor"), List.of("config", "show"),
                List.of("config", "validate"), List.of("rule", "validate"), List.of("standard", "validate")))
            assertEquals(600_000L, RfInvoker.processTimeoutMs(args, 5));
        assertEquals(75L, RfInvoker.processTimeoutMs(List.of("scan", "--deadline-ms", "75"), 5));
        assertEquals(5L, RfInvoker.processTimeoutMs(List.of("feedback", "submit"), 5));
        assertEquals(5L, RfInvoker.processTimeoutMs(List.of("registry", "list"), 5));
        for (String deadline : List.of("0", "-1", "abc", "999999999999999999999")) {
            try { RfInvoker.processTimeoutMs(List.of("scan", "--deadline-ms", deadline), 5); fail(deadline); }
            catch (IllegalArgumentException expected) { assertTrue(expected.getMessage().contains("DEADLINE")); }
        }
        try { RfInvoker.processTimeoutMs(List.of("scan", "--deadline-ms"), 5); fail("missing deadline"); }
        catch (IllegalArgumentException expected) { assertEquals("INVALID_DEADLINE_MS", expected.getMessage()); }
    }

    public void testProgressArrivesBeforeExitWithUtf8CrLfAndFinalPartialLine() throws Exception {
        Path directory = Files.createTempDirectory("erf-progress-");
        Path source = directory.resolve("Progress.java"), acknowledged = directory.resolve("seen");
        Files.writeString(source, """
            import java.nio.file.*;
            class Progress {
                public static void main(String[] args) throws Exception {
                    System.err.print("progress 中文\\r\\n"); System.err.flush();
                    long until = System.nanoTime() + 3_000_000_000L;
                    while (!Files.exists(Path.of(args[0])) && System.nanoTime() < until) Thread.sleep(10);
                    if (!Files.exists(Path.of(args[0]))) throw new IllegalStateException("NO_LIVE_PROGRESS");
                    System.err.print("尾行"); System.out.print("done");
                }
            }
            """);
        AtomicBoolean complete = new AtomicBoolean();
        AtomicReference<Exception> error = new AtomicReference<>();
        AtomicReference<RfInvoker.Result> receipt = new AtomicReference<>();
        List<String> lines = java.util.Collections.synchronizedList(new ArrayList<>());
        var invoker = getProject().getService(RfInvoker.class);
        var job = invoker.submit(getProject(), indicator -> {
            assertTrue(indicator.isIndeterminate());
            String java = Path.of(System.getProperty("java.home"), "bin", System.getProperty("os.name").startsWith("Windows") ? "java.exe" : "java").toString();
            return invoker.run(java, List.of(source.toString(), acknowledged.toString()), directory.toString(), 10_000, indicator, line -> {
                assertFalse(ApplicationManager.getApplication().isDispatchThread()); lines.add(line);
                if (line.equals("progress 中文")) try { Files.writeString(acknowledged, "seen"); }
                catch (java.io.IOException ex) { throw new UncheckedIOException(ex); }
            });
        }, result -> { receipt.set(result); complete.set(true); },
            ex -> { error.set(ex); complete.set(true); }, () -> { complete.set(true); fail("unexpected cancellation"); });
        try {
            await(complete); if (error.get() != null) throw error.get();
            assertNotNull(receipt.get());
            assertEquals(0, receipt.get().exitCode()); assertEquals("done", receipt.get().stdout());
            assertEquals(List.of("progress 中文", "尾行"), lines);
            assertEquals("progress 中文\r\n尾行", receipt.get().stderr());
        }
        finally { job.cancel(); Files.deleteIfExists(acknowledged); Files.deleteIfExists(source); Files.deleteIfExists(directory); }
    }

    public void testMissingFindingIsDisabledAcrossAllFindingScenesAndFileChanges() {
        var panel = new ErfToolWindowFactory.Panel(getProject());
        try {
            for (var scene : ErfToolWindowFactory.Scene.values()) {
                if (!scene.needsFinding()) continue;
                panel.select(scene);
                JButton execute = find(panel, JButton.class, b -> b.getText().startsWith("预览并"));
                assertNotNull(execute); assertFalse(scene.name(), execute.isEnabled());
            }
            panel.select(ErfToolWindowFactory.Scene.SHOW);
            JTextField file = find(panel, JTextField.class, f -> "FindingView 文件路径".equals(f.getAccessibleContext().getAccessibleName()));
            assertNotNull(file);
            file.setText("   ");
            assertFalse(find(panel, JButton.class, b -> b.getText().startsWith("预览并")).isEnabled());
            file.setText("finding.json");
            if (getProject().getBasePath() != null)
                assertTrue(find(panel, JButton.class, b -> b.getText().startsWith("预览并")).isEnabled());
            file.setText("");
            assertFalse(find(panel, JButton.class, b -> b.getText().startsWith("预览并")).isEnabled());
        } finally { panel.dispose(); }
    }

    public void testNativeSceneSearchFindsCollapsedScenesWithoutExecuting() {
        var panel = new ErfToolWindowFactory.Panel(getProject());
        try {
            JTree tree = find(panel, JTree.class, value -> true);
            for (int row = tree.getRowCount() - 1; row >= 0; row--) tree.collapseRow(row);
            var search = com.intellij.ui.speedSearch.SpeedSearchSupply.getSupply(tree, true);
            assertNotNull(search);
            search.findAndSelectElement("校验规则文件");
            var selected = (DefaultMutableTreeNode) tree.getLastSelectedPathComponent();
            assertNotNull(selected);
            assertEquals(ErfToolWindowFactory.Scene.RULE_VALIDATE, selected.getUserObject());
            assertTrue(tree.isVisible(new TreePath(selected.getPath())));
            assertNotNull(find(panel, JButton.class, button -> "预览并执行".equals(button.getText())));
        } finally { panel.dispose(); }
    }

    public void testNativeScrollableFormAccessibilityAndEscape() {
        var panel = new ErfToolWindowFactory.Panel(getProject());
        try {
            panel.select(ErfToolWindowFactory.Scene.SCAN);
            JBScrollPane form = find(panel, JBScrollPane.class, scroll -> scroll.getViewport().getView() instanceof JPanel);
            assertNotNull(form); assertEquals(ScrollPaneConstants.HORIZONTAL_SCROLLBAR_NEVER, form.getHorizontalScrollBarPolicy());
            assertNotNull(find(form, JBTextField.class, input -> true));
            assertEquals("text", find(form, JComboBox.class, input -> "进度输出".equals(input.getAccessibleContext().getAccessibleName())).getSelectedItem());
            assertEquals("json", find(form, JComboBox.class, input -> "输出格式".equals(input.getAccessibleContext().getAccessibleName())).getSelectedItem());
            assertNotNull(find(panel, JBTextArea.class, input -> "执行回执与输出日志".equals(input.getAccessibleContext().getAccessibleName())));
            JLabel status = find(panel, JLabel.class, label -> label.getAccessibleContext().getAccessibleRole() == AccessibleRole.STATUS_BAR);
            assertNotNull(status);
            AtomicBoolean changed = new AtomicBoolean();
            status.getAccessibleContext().addPropertyChangeListener(event -> {
                if (AccessibleContext.ACCESSIBLE_VISIBLE_DATA_PROPERTY.equals(event.getPropertyName())) changed.set(true);
            });
            panel.select(ErfToolWindowFactory.Scene.SHOW);
            assertTrue(changed.get());
            assertNotNull(panel.getInputMap(JComponent.WHEN_ANCESTOR_OF_FOCUSED_COMPONENT).get(KeyStroke.getKeyStroke("ESCAPE")));
            assertNotNull(find(panel, JButton.class, button -> "预览并执行".equals(button.getText())).getIcon());
            panel.select(ErfToolWindowFactory.Scene.FEEDBACK);
            assertNotNull(find(panel, JTextField.class, input -> "提交者".equals(input.getAccessibleContext().getAccessibleName())));
            assertNotNull(find(panel, JTextArea.class, input -> input.getAccessibleContext().getAccessibleName().startsWith("理由")));
        } finally { panel.dispose(); }
    }

    public void testRunHistoryRestoresReceiptAndTimestampWithoutExecuting() {
        var panel = new ErfToolWindowFactory.Panel(getProject());
        try {
            JTree tree = find(panel, JTree.class, value -> true);
            var root = (DefaultMutableTreeNode) tree.getModel().getRoot();
            var runs = (DefaultMutableTreeNode) root.getLastChild();
            var record = new ErfToolWindowFactory.RunRecord(ErfToolWindowFactory.Scene.SCAN, "完成", "receipt snapshot", Instant.parse("2026-10-05T00:00:00Z"));
            var node = new DefaultMutableTreeNode(record); runs.add(node);
            tree.setSelectionPath(new TreePath(node.getPath()));
            JTextArea output = find(panel, JTextArea.class, area -> "执行回执与输出日志".equals(area.getAccessibleContext().getAccessibleName()));
            assertTrue(output.getText().contains("receipt snapshot"));
            assertTrue(output.getText().contains("2026-10-05T00:00:00Z"));
            assertTrue(output.getText().contains("扫描结果工作台"));
        } finally { panel.dispose(); }
    }

    public void testSettingsUseLabeledNativeFormRatherThanEqualWidthGrid() {
        JComponent panel = new ErfConfigurable(getProject()).createComponent();
        assertFalse(panel.getLayout() instanceof java.awt.GridLayout);
        assertNotNull(find(panel, JBTextField.class, field -> "rf 启动器路径".equals(field.getAccessibleContext().getAccessibleName())));
    }

    private static <T extends Component> T find(Component component, Class<T> type, Predicate<T> matches) {
        if (type.isInstance(component) && matches.test(type.cast(component))) return type.cast(component);
        if (component instanceof Container container) for (Component child : container.getComponents()) {
            T found = find(child, type, matches); if (found != null) return found;
        }
        return null;
    }

    private static boolean hasPlainTextRegistryRenderer(java.awt.Component component) {
        if (component instanceof javax.swing.JComboBox<?> combo && combo.getRenderer() instanceof javax.swing.JComponent renderer
            && Boolean.TRUE.equals(renderer.getClientProperty("html.disable"))) return true;
        if (component instanceof java.awt.Container container)
            for (var child : container.getComponents()) if (hasPlainTextRegistryRenderer(child)) return true;
        return false;
    }

    private static void await(AtomicBoolean completed) throws Exception {
        long deadline = System.nanoTime() + 15_000_000_000L;
        while (!completed.get() && System.nanoTime() < deadline) {
            UIUtil.dispatchAllInvocationEvents(); Thread.sleep(10);
        }
        assertTrue("background callback did not arrive", completed.get());
    }
}
