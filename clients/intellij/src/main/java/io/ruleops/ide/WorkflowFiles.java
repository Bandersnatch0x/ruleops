package io.ruleops.ide;

import io.ruleops.ide.core.ThinClientCore;
import java.io.IOException;
import java.nio.file.*;
import java.nio.file.attribute.FileTime;
import java.util.*;

/** Filesystem evidence lives here, never in the pure option projection. No workspace scanning. */
public final class WorkflowFiles {
    private WorkflowFiles() {}
    public static Path checkedPath(Path path, boolean directory) throws IOException {
        Path absolute = path.toAbsolutePath().normalize();
        Path cursor = absolute.getRoot();
        for (Path part : absolute) {
            cursor = cursor.resolve(part);
            if (Files.isSymbolicLink(cursor)) throw new IOException("SYMLINK_COMPONENT:" + cursor);
        }
        if (directory ? !Files.isDirectory(absolute, LinkOption.NOFOLLOW_LINKS) : !Files.isRegularFile(absolute, LinkOption.NOFOLLOW_LINKS))
            throw new IOException("MISSING_OR_WRONG_FILE_TYPE:" + absolute);
        return absolute.toRealPath();
    }
    public static Path workspacePath(Path root, String relative, boolean directory) throws IOException {
        if (!ThinClientCore.validateRelativePath(relative).ok || relative.matches(".*[\\*?\\[\\]{}!].*"))
            throw new IOException("INVALID_WORKSPACE_PATH");
        Path base = checkedPath(root, true);
        Path target = base.resolve(relative).normalize();
        if (!target.startsWith(base)) throw new IOException("TARGET_ESCAPES_WORKSPACE");
        Path real = checkedPath(target, directory);
        if (!real.startsWith(base)) throw new IOException("REAL_TARGET_ESCAPES_WORKSPACE");
        return real;
    }
    public record ScopeEvidence(List<ThinClientCore.ScopeOption> options, List<String> rejected) {}
    public static ScopeEvidence scopes(Path root, Map<String, Object> finding, Map<String, ?> config) throws IOException {
        ThinClientCore.Check check = ThinClientCore.validateFindingView(finding);
        if (!check.ok) throw new IOException("INVALID_FINDING:" + check.reason);
        String file = (String) ((Map<?, ?>) finding.get("location")).get("path");
        workspacePath(root, file, false);
        Set<String> candidates = new LinkedHashSet<>();
        int slash = file.lastIndexOf('/');
        if (slash > 0) candidates.add(file.substring(0, slash));
        for (String field : List.of("modules", "source_roots")) {
            Object values = config.get(field);
            if (!(values instanceof List<?> list)) throw new IOException("INVALID_EFFECTIVE_CONFIG:" + field);
            for (Object value : list) {
                if (!(value instanceof String path)) throw new IOException("INVALID_EFFECTIVE_CONFIG_PATH");
                if (file.startsWith(path + "/")) candidates.add(path);
            }
        }
        List<String> valid = new ArrayList<>(), rejected = new ArrayList<>();
        for (String candidate : candidates) {
            try { workspacePath(root, candidate, true); valid.add(candidate); }
            catch (IOException error) { rejected.add(candidate + ": " + error.getMessage()); }
        }
        return new ScopeEvidence(ThinClientCore.scopeOptions(finding, config, List.of(file), valid), List.copyOf(rejected));
    }
    public record OutputStamp(Path path, FileTime previousModified, boolean existed) {}
    public static OutputStamp prepareOutput(Path target, boolean overwriteConfirmed, List<Path> inputs) throws IOException {
        Path absolute = target.toAbsolutePath().normalize();
        checkedPath(absolute.getParent(), true);
        if (Files.isSymbolicLink(absolute)) throw new IOException("OUTPUT_SYMLINK");
        for (Path input : inputs) if (absolute.equals(input.toAbsolutePath().normalize())
                || (Files.exists(absolute) && Files.isSameFile(absolute, input))) throw new IOException("OUTPUT_OVERWRITES_INPUT");
        boolean exists = Files.exists(absolute, LinkOption.NOFOLLOW_LINKS);
        if (exists) {
            checkedPath(absolute, false);
            if (!overwriteConfirmed) throw new IOException("OUTPUT_OVERWRITE_CONFIRMATION_REQUIRED");
        }
        return new OutputStamp(absolute, exists ? Files.getLastModifiedTime(absolute) : null, exists);
    }
    public static Path verifyWritten(OutputStamp stamp) throws IOException {
        Path path = checkedPath(stamp.path(), false);
        if (Files.size(path) == 0 || (stamp.existed() && Files.getLastModifiedTime(path).equals(stamp.previousModified())))
            throw new IOException("OUTPUT_NOT_WRITTEN");
        return path;
    }
}
