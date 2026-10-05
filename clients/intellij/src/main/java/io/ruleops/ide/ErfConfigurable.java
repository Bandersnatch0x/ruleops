package io.ruleops.ide;

import com.intellij.openapi.options.Configurable;
import com.intellij.openapi.options.ConfigurationException;
import com.intellij.openapi.project.Project;
import javax.swing.*;
import com.intellij.ui.components.JBTextField;
import com.intellij.util.ui.FormBuilder;
import com.intellij.util.ui.JBUI;

public final class ErfConfigurable implements Configurable {
    private final Project project;
    private final JTextField rfPath = new JBTextField();
    private final JTextField endpoint = new JBTextField();
    private final JTextField connectionProfile = new JBTextField();
    private final JSpinner timeout = new JSpinner(new SpinnerNumberModel(5000, 1, Integer.MAX_VALUE, 100));
    private final JCheckBox telemetry = new JCheckBox("允许遥测（默认关闭）");

    public ErfConfigurable(Project project) { this.project = project; }
    @Override public String getDisplayName() { return "ERF"; }
    @Override public JComponent createComponent() {
        rfPath.getAccessibleContext().setAccessibleName("rf 启动器路径");
        endpoint.getAccessibleContext().setAccessibleName("Server endpoint（显式配置）");
        connectionProfile.getAccessibleContext().setAccessibleName("Registry connection profile FILE（仅路径，不填凭据）");
        timeout.getAccessibleContext().setAccessibleName("请求超时（毫秒，仅短请求）");
        JPanel panel = FormBuilder.createFormBuilder()
            .addLabeledComponent("rf 启动器路径", rfPath)
            .addLabeledComponent("Server endpoint（显式配置）", endpoint)
            .addLabeledComponent("Registry connection profile FILE（仅路径，不填凭据）", connectionProfile)
            .addLabeledComponent("请求超时（毫秒，仅短请求）", timeout)
            .addComponent(telemetry)
            .addTooltip("连接安全与业务校验由 CLI 定义；本地扫描/诊断/校验默认最多 600 秒，扫描可指定 deadlineMs。")
            .addComponentFillVertically(new JPanel(), 0)
            .getPanel();
        panel.setBorder(JBUI.Borders.empty(8));
        reset();
        return panel;
    }
    @Override public boolean isModified() {
        ErfSettings.State s = project.getService(ErfSettings.class).getState();
        return !rfPath.getText().equals(s.rfPath) || !endpoint.getText().equals(s.serverEndpoint)
            || !connectionProfile.getText().equals(s.connectionProfile)
            || !timeout.getValue().equals(s.requestTimeoutMs) || telemetry.isSelected() != s.telemetryEnabled;
    }
    @Override public void apply() throws ConfigurationException {
        if (rfPath.getText().isBlank()) throw new ConfigurationException("请配置 rf 启动器路径");
        ErfSettings.State s = new ErfSettings.State();
        s.rfPath = rfPath.getText().trim(); s.serverEndpoint = endpoint.getText().trim();
        s.connectionProfile = connectionProfile.getText().trim();
        s.requestTimeoutMs = ((Number) timeout.getValue()).intValue();
        s.telemetryEnabled = telemetry.isSelected();
        project.getService(ErfSettings.class).loadState(s);
        ErfToolWindowFactory.settingsChanged(project);
    }
    @Override public void reset() {
        ErfSettings.State s = project.getService(ErfSettings.class).getState();
        rfPath.setText(s.rfPath); endpoint.setText(s.serverEndpoint);
        connectionProfile.setText(s.connectionProfile);
        timeout.setValue(s.requestTimeoutMs); telemetry.setSelected(s.telemetryEnabled);
    }
}
