package io.ruleops.ide.actions;

import com.intellij.openapi.actionSystem.AnAction;
import com.intellij.openapi.actionSystem.AnActionEvent;
import io.ruleops.ide.ErfToolWindowFactory;

public abstract class AbstractErfAction extends AnAction {
    private final ErfToolWindowFactory.Scene scene;
    protected AbstractErfAction(ErfToolWindowFactory.Scene scene) { this.scene = scene; }
    @Override public final void actionPerformed(AnActionEvent event) {
        if (event.getProject() != null) ErfToolWindowFactory.open(event.getProject(), scene);
    }
}
