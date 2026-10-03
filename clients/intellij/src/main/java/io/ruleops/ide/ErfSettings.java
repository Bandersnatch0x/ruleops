package io.ruleops.ide;

// 客户端配置：rf 路径、可选 Server endpoint、遥测开关（默认关）、请求超时。
// 与 clients/vscode/package.json 的 erf.* 配置项对齐。

import com.intellij.openapi.components.PersistentStateComponent;
import com.intellij.openapi.components.State;
import com.intellij.openapi.components.Storage;
import com.intellij.openapi.components.Service;
import org.jetbrains.annotations.NotNull;

@Service(Service.Level.PROJECT)
@State(name = "ErfThinClientSettings", storages = @Storage("erf-thin-client.xml"))
public final class ErfSettings implements PersistentStateComponent<ErfSettings.State> {
    public static final class State {
        public String rfPath = "rf";
        public String serverEndpoint = "";
        public boolean telemetryEnabled = false;
        public int requestTimeoutMs = 5000;
    }

    private State state = new State();

    public State snapshot() {
        State copy = new State();
        copy.rfPath = state.rfPath;
        copy.serverEndpoint = state.serverEndpoint;
        copy.telemetryEnabled = state.telemetryEnabled;
        copy.requestTimeoutMs = state.requestTimeoutMs;
        return copy;
    }

    @Override
    public @NotNull State getState() {
        return state;
    }

    @Override
    public void loadState(@NotNull State s) {
        this.state = s;
    }
}
