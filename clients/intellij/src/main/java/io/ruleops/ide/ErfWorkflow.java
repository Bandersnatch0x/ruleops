package io.ruleops.ide;

import com.google.gson.*;
import com.intellij.openapi.progress.ProgressIndicator;
import io.ruleops.ide.actions.FindingViewLoader;
import io.ruleops.ide.core.ThinClientCore;
import io.ruleops.ide.ErfToolWindowFactory.Scene;
import java.nio.file.*;
import java.security.MessageDigest;
import java.time.Instant;
import java.util.*;

/** Background-only workflow: CLI execution and filesystem evidence, not execution authority. */
final class ErfWorkflow {
    private ErfWorkflow() {}
    record Request(Scene scene, ErfSettings.State config, String base, String findingPath,
                   String actor, String reason, String classification, String scope, boolean broaderConfirmed,
                   String result, String authorityTrust, ErfArtifactIndex.Entry artifact, boolean byDigest,
                   String output, boolean overwriteConfirmed) {}
    record Result(String detail, Path target, int line, int column, ErfArtifactIndex.Entry artifact) {
        Result(String detail, ErfArtifactIndex.Entry artifact) { this(detail, null, 0, 0, artifact); }
    }
    static List<String> args(Request r) {
        return switch (r.scene()) {
            case EXPLAIN -> {
                if (r.result().isBlank() != r.authorityTrust().isBlank()) throw new IllegalArgumentException("RESULT_AND_AUTHORITY_TRUST_MUST_BE_PAIRED");
                var opts = new ThinClientCore.ExplainOpts(); opts.format = "text";
                opts.result = r.result(); opts.authorityTrust = r.authorityTrust();
                yield ThinClientCore.buildExplainArgs(r.findingPath(), opts);
            }
            case FEEDBACK -> ThinClientCore.buildFeedbackDraftArgs(r.findingPath(), r.classification(), r.reason(), r.actor(), r.output());
            case EXCEPTION -> ThinClientCore.buildExceptionRequestArgs(r.findingPath(), r.scope(), r.reason(), r.actor(), r.output());
            case SUBMIT_FEEDBACK, SUBMIT_EXCEPTION -> ThinClientCore.buildSubmitArgs(
                r.scene() == Scene.SUBMIT_FEEDBACK ? "feedback" : "exception", r.artifact().path,
                checkedEndpoint(r.config().serverEndpoint), r.config().requestTimeoutMs, r.output());
            case STATUS -> ThinClientCore.buildSubmissionStatusArgs(checkedEndpoint(r.config().serverEndpoint),
                r.byDigest() ? null : r.artifact().requestId, r.byDigest() ? r.artifact().artifactDigest : null, r.config().requestTimeoutMs);
            default -> List.of();
        };
    }
    static String checkedEndpoint(String endpoint) {
        java.net.URI uri;
        try { uri = java.net.URI.create(endpoint); }
        catch (IllegalArgumentException error) { throw new IllegalArgumentException("INVALID_ENDPOINT"); }
        if (uri.getHost() == null || !List.of("http", "https").contains(uri.getScheme())
            || uri.getUserInfo() != null || uri.getQuery() != null || uri.getFragment() != null)
            throw new IllegalArgumentException("ENDPOINT_REQUIRES_EXPLICIT_URL_WITHOUT_CREDENTIALS_QUERY_OR_FRAGMENT");
        return endpoint;
    }
    static Map<String, Object> finding(String path) throws Exception {
        var loaded = FindingViewLoader.load(WorkflowFiles.checkedPath(Path.of(path), false));
        if (!loaded.check.ok) throw new IllegalArgumentException("FindingView 被拒绝：" + loaded.check.reason);
        return loaded.view;
    }
    static WorkflowFiles.ScopeEvidence scopes(RfInvoker invoker, ProgressIndicator indicator, ErfSettings.State config,
                                               String base, String findingPath) throws Exception {
        var view = finding(findingPath);
        RfInvoker.Result result = invoker.run(config.rfPath, List.of("config", "show", "--repo-root", base, "--format", "json"), base, config.requestTimeoutMs, indicator);
        requireExit(result);
        Object inner = object(result.stdout()).get("config");
        if (!(inner instanceof Map<?, ?> map)) throw new IllegalArgumentException("INVALID_EFFECTIVE_CONFIG");
        @SuppressWarnings("unchecked") Map<String, Object> effective = (Map<String, Object>) map;
        return WorkflowFiles.scopes(Path.of(base), view, effective);
    }
    static Result perform(RfInvoker invoker, ProgressIndicator indicator, Request r) throws Exception {
        indicator.checkCanceled();
        Map<String, Object> view = r.scene().server() ? null : finding(r.findingPath());
        if (r.scene() == Scene.SHOW) return new Result(new Gson().toJson(view), null);
        if (r.scene() == Scene.NAVIGATE) {
            Map<?, ?> location = (Map<?, ?>) view.get("location");
            String relative = (String) location.get("path");
            Path target = WorkflowFiles.workspacePath(Path.of(r.base()), relative, false);
            return new Result(relative, target, ((Number) location.get("line")).intValue() - 1,
                ((Number) location.get("column")).intValue() - 1, null);
        }
        if (r.scene() == Scene.EXCEPTION) {
            var available = scopes(invoker, indicator, r.config(), r.base(), r.findingPath());
            ThinClientCore.selectScope(available.options(), r.scope(), r.broaderConfirmed());
        }
        if (r.scene().server()) validateSelected(r);
        List<Path> inputs = new ArrayList<>();
        if (r.scene().server()) inputs.add(Path.of(r.artifact().path));
        else {
            inputs.add(Path.of(r.findingPath()));
            String relative = (String) ((Map<?, ?>) view.get("location")).get("path");
            Path source = Path.of(r.base()).resolve(relative).toAbsolutePath().normalize();
            if (Files.exists(source, LinkOption.NOFOLLOW_LINKS)) inputs.add(source);
        }
        if (!r.result().isBlank()) inputs.add(WorkflowFiles.checkedPath(Path.of(r.result()), false));
        if (!r.authorityTrust().isBlank()) inputs.add(WorkflowFiles.checkedPath(Path.of(r.authorityTrust()), false));
        WorkflowFiles.OutputStamp stamp = r.output().isBlank() ? null
            : WorkflowFiles.prepareOutput(Path.of(r.output()), r.overwriteConfirmed(), inputs);
        RfInvoker.Result result = invoker.run(r.config().rfPath, args(r), r.base(), r.config().requestTimeoutMs, indicator);
        // Status exit 3 carries an UNKNOWN/PENDING response, not a successful submission.
        if (result.exitCode() != 0 && !(r.scene() == Scene.STATUS && result.exitCode() == 3)) requireExit(result);
        String text = stamp == null ? result.stdout() : Files.readString(WorkflowFiles.verifyWritten(stamp));
        if (r.scene() == Scene.EXPLAIN) return new Result("exit=" + result.exitCode() + "\n" + text, null);
        Map<String, Object> object = object(text);
        if (r.scene() == Scene.FEEDBACK || r.scene() == Scene.EXCEPTION) {
            if (stamp == null) throw new IllegalArgumentException("EXPLICIT_OUTPUT_REQUIRED");
            ThinClientCore.Artifact identity = ThinClientCore.savedArtifact(object);
            String expectedKind = r.scene() == Scene.FEEDBACK ? "FeedbackDraft" : "ExceptionRequest";
            String field = r.scene() == Scene.FEEDBACK ? "classification" : "scope";
            if (!expectedKind.equals(identity.kind()) || !r.actor().equals(object.get("actor")) || !r.reason().equals(object.get("reason"))
                || !(r.scene() == Scene.FEEDBACK ? r.classification() : r.scope()).equals(object.get(field))
                || !view.get("ruleId").equals(object.get("ruleId")) || !view.get("fingerprint").equals(object.get("findingFingerprint")))
                throw new IllegalArgumentException("SAVED_OBJECT_DOES_NOT_MATCH_FORM");
            var entry = new ErfArtifactIndex.Entry(); entry.workspace = r.base(); entry.path = stamp.path().toString();
            entry.kind = identity.kind(); entry.requestId = identity.requestId(); entry.artifactDigest = identity.artifactDigest();
            entry.status = identity.status(); entry.savedAt = Instant.now().toString(); entry.contentHash = hash(Path.of(entry.path));
            return new Result("实际已保存：" + entry.path + "\n" + ThinClientCore.statusLabel(entry.status) + "\n" + text, entry);
        }
        String state = ThinClientCore.checkedResponse(object, r.artifact().identity(), r.scene() == Scene.STATUS);
        var entry = copy(r.artifact()); entry.status = state;
        if (stamp != null) entry.receiptPath = stamp.path().toString();
        return new Result("exit=" + result.exitCode() + "\n" + ThinClientCore.statusLabel(state.equals("SUBMITTED") ? "RECEIVED_ONLY" : state)
            + "\n仅接收/记录，非采纳、非批准、非 Gate PASS、非 SCM 写入。\n回执：" + entry.receiptPath + "\n" + text, entry);
    }
    static void validateSelected(Request r) throws Exception {
        var entry = r.artifact();
        if (entry == null || !r.base().equals(entry.workspace) || !r.config().serverEndpoint.equals(entry.endpoint))
            throw new IllegalArgumentException("STALE_ARTIFACT_SELECTION");
        Path file = WorkflowFiles.checkedPath(Path.of(entry.path), false);
        ThinClientCore.Artifact actual = ThinClientCore.savedArtifact(object(Files.readString(file)));
        if (!entry.contentHash.equals(hash(file)) || !actual.requestId().equals(entry.requestId)
            || !actual.artifactDigest().equals(entry.artifactDigest) || !actual.kind().equals(entry.kind))
            throw new IllegalArgumentException("SAVED_ARTIFACT_CHANGED_RESELECT_OR_RECREATE");
        if ((r.scene() == Scene.SUBMIT_FEEDBACK && !"FeedbackDraft".equals(entry.kind))
            || (r.scene() == Scene.SUBMIT_EXCEPTION && !"ExceptionRequest".equals(entry.kind)))
            throw new IllegalArgumentException("WRONG_SUBMISSION_KIND");
    }
    static ErfArtifactIndex.Entry copy(ErfArtifactIndex.Entry entry) {
        return new Gson().fromJson(new Gson().toJson(entry), ErfArtifactIndex.Entry.class);
    }
    private static String hash(Path path) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(path)));
    }
    @SuppressWarnings("unchecked") static Map<String, Object> object(String text) {
        Object parsed = new Gson().fromJson(text, Object.class);
        if (!(parsed instanceof Map<?, ?>)) throw new IllegalArgumentException("EXPECTED_JSON_OBJECT");
        return (Map<String, Object>) parsed;
    }
    private static void requireExit(RfInvoker.Result result) {
        if (result.exitCode() != 0) throw new IllegalStateException(ThinClientCore.classifyExitCode(result.exitCode())
            + " · exit=" + result.exitCode() + "\n" + result.stderr());
    }
}
