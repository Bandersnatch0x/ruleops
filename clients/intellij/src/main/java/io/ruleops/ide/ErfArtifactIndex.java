package io.ruleops.ide;

import com.intellij.openapi.components.*;
import io.ruleops.ide.core.ThinClientCore;
import org.jetbrains.annotations.NotNull;
import java.util.ArrayList;
import java.util.List;

/** Project-local metadata only; requests/receipts remain explicit files, never new authority. */
@Service(Service.Level.PROJECT)
@State(name = "ErfArtifactIndex", storages = @Storage(StoragePathMacros.WORKSPACE_FILE))
public final class ErfArtifactIndex implements PersistentStateComponent<ErfArtifactIndex.State> {
    public static final class Entry {
        public String workspace = "", path = "", kind = "", requestId = "", artifactDigest = "";
        public String endpoint = "", status = "", savedAt = "", receiptPath = "", contentHash = "";
        public ThinClientCore.Artifact identity() { return new ThinClientCore.Artifact(kind, requestId, artifactDigest, status); }
        @Override public String toString() { return kind + " · " + savedAt + " · " + status + " · " + (endpoint.isEmpty() ? "本地未提交" : endpoint) + " · " + requestId; }
    }
    public static final class State { public List<Entry> entries = new ArrayList<>(); }
    private State state = new State();
    @Override public @NotNull State getState() { return state; }
    @Override public void loadState(@NotNull State next) { state = next; }
    public List<Entry> entries() { return List.copyOf(state.entries); }
    public void remember(Entry entry) {
        state.entries.removeIf(old -> old.workspace.equals(entry.workspace) && old.path.equals(entry.path) && old.endpoint.equals(entry.endpoint));
        state.entries.add(0, entry);
    }
}
