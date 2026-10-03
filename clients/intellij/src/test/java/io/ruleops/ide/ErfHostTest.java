package io.ruleops.ide;

import com.intellij.openapi.application.ApplicationManager;
import com.intellij.testFramework.LightPlatformTestCase;
import com.intellij.util.ui.UIUtil;
import javax.xml.parsers.DocumentBuilderFactory;
import java.nio.file.Path;
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
            assertEquals(8, ErfToolWindowFactory.Scene.values().length);
        } finally { panel.dispose(); }
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

    private static void await(AtomicBoolean completed) throws Exception {
        long deadline = System.nanoTime() + 5_000_000_000L;
        while (!completed.get() && System.nanoTime() < deadline) {
            UIUtil.dispatchAllInvocationEvents(); Thread.sleep(10);
        }
        assertTrue("background callback did not arrive", completed.get());
    }
}
