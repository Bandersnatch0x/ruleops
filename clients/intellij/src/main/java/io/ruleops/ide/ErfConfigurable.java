package io.ruleops.ide;

import com.intellij.openapi.options.Configurable;
import com.intellij.openapi.options.ConfigurationException;
import com.intellij.openapi.project.Project;
import javax.swing.*;
import java.awt.*;

public final class ErfConfigurable implements Configurable {
    private final Project project;
    private final JTextField rfPath = new JTextField();
    private final JTextField endpoint = new JTextField();
    private final JTextField connectionProfile = new JTextField();
    private final JSpinner timeout = new JSpinner(new SpinnerNumberModel(5000, 1, Integer.MAX_VALUE, 100));
    private final JCheckBox telemetry = new JCheckBox("允许遥测（默认关闭）");

    public ErfConfigurable(Project project) { this.project = project; }
    @Override public String getDisplayName() { return "ERF"; }
    @Override public JComponent createComponent() {
        JPanel panel = new JPanel(new GridLayout(0, 2, 8, 8));
        panel.add(new JLabel("rf 启动器路径")); panel.add(rfPath);
        panel.add(new JLabel("Server endpoint（显式配置）")); panel.add(endpoint);
        panel.add(new JLabel("Registry connection profile FILE（仅路径，不填凭据）")); panel.add(connectionProfile);
        panel.add(new JLabel("请求超时（毫秒）")); panel.add(timeout);
        panel.add(telemetry); panel.add(new JLabel("连接安全与业务校验由 CLI 定义"));
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
