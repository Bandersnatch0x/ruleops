package io.ruleops.authoring.orchestration;

import io.ruleops.authoring.suggestion.AuthoringSuggestion;
import io.ruleops.authoring.suggestion.AuthoringSuggestion.Suggestion;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;

/**
 * Deterministic, non-authoritative authoring task orchestration.
 *
 * <p>The orchestrator freezes the task list before invoking a capability,
 * partitions it by stable task id, and records one terminal state for every
 * planned task. A failed group is retried through the single-task fallback so
 * a capability failure cannot silently reduce coverage. No class in this
 * package can mutate Rule maturity, policy, release or Gate state.</p>
 */
public final class AuthoringOrchestrator {

    public static final String CAPABILITY = "authoring-orchestrator";
    public static final String CAPABILITY_VERSION = "0.1.0";

    @FunctionalInterface
    public interface GroupExecutor {
        Execution execute(List<AuthoringTask> tasks) throws Exception;
    }

    @FunctionalInterface
    public interface FallbackExecutor {
        Execution execute(AuthoringTask task) throws Exception;
    }

    public enum AttemptState {
        SUCCESS,
        EMPTY,
        TIMED_OUT,
        REJECTED,
        FAILED
    }

    public enum TaskTerminalState {
        PRIMARY_COMPLETED,
        FALLBACK_COMPLETED,
        EMPTY,
        TIMED_OUT,
        REJECTED,
        FAILED
    }

    public enum OverallState {
        COMPLETED,
        DEGRADED,
        FAILED,
        REJECTED
    }

    /** Frozen request input; all fields are copied and never reordered in place. */
    public record Request(
            AuthoringSuggestion.Source source,
            AuthoringSuggestion.Scope scope,
            List<AuthoringTask> tasks,
            Set<String> permittedSourceRefs,
            int groupSize) {

        public Request {
            source = Objects.requireNonNull(source, "source");
            scope = Objects.requireNonNull(scope, "scope");
            if (tasks == null || tasks.isEmpty()) {
                throw new IllegalArgumentException("tasks must not be empty");
            }
            tasks = List.copyOf(tasks);
            if (permittedSourceRefs == null || permittedSourceRefs.isEmpty()) {
                throw new IllegalArgumentException("permittedSourceRefs must not be empty");
            }
            permittedSourceRefs = Set.copyOf(permittedSourceRefs);
            if (groupSize < 1 || groupSize > 128) {
                throw new IllegalArgumentException("groupSize must be between 1 and 128");
            }
        }
    }

    public record AuthoringTask(
            String taskId,
            String standardId,
            TaskType type,
            List<String> inputRefs) {

        public AuthoringTask {
            requiredText("taskId", taskId);
            requiredText("standardId", standardId);
            type = Objects.requireNonNull(type, "type");
            if (inputRefs == null || inputRefs.isEmpty()) {
                throw new IllegalArgumentException("inputRefs must not be empty");
            }
            inputRefs = inputRefs.stream().map(value -> {
                requiredText("inputRef", value);
                return value;
            }).distinct().sorted().toList();
        }
    }

    public enum TaskType {
        SCENARIO,
        POSITIVE,
        NEGATIVE,
        NEAR_MISS,
        EXPLANATION,
        RELATED_ASSET
    }

    /** Capability response for one group or one fallback task. */
    public record Execution(
            AttemptState state,
            Map<String, List<Suggestion>> suggestions,
            String reason) {

        public Execution {
            state = Objects.requireNonNull(state, "state");
            if (suggestions == null) {
                throw new IllegalArgumentException("suggestions must not be null");
            }
            Map<String, List<Suggestion>> copied = new LinkedHashMap<>();
            for (Map.Entry<String, List<Suggestion>> entry : suggestions.entrySet()) {
                requiredText("taskId", entry.getKey());
                if (entry.getValue() == null) {
                    throw new IllegalArgumentException("suggestion list must not be null");
                }
                copied.put(entry.getKey(), List.copyOf(entry.getValue()));
            }
            suggestions = Map.copyOf(copied);
            reason = reason == null ? "" : reason;
            if (reason.chars().anyMatch(Character::isISOControl)) {
                throw new IllegalArgumentException("reason must be control-free");
            }
            if (state != AttemptState.SUCCESS && !suggestions.isEmpty()) {
                throw new IllegalArgumentException("non-success execution cannot carry suggestions");
            }
        }

        public static Execution success(String taskId, List<Suggestion> suggestions) {
            return new Execution(AttemptState.SUCCESS, Map.of(taskId, List.copyOf(suggestions)), "");
        }

        public static Execution failure(AttemptState state, String reason) {
            if (state == AttemptState.SUCCESS) {
                throw new IllegalArgumentException("failure state must not be SUCCESS");
            }
            return new Execution(state, Map.of(), reason);
        }
    }

    public record TaskOutcome(
            String taskId,
            TaskTerminalState state,
            List<Suggestion> suggestions,
            String reason) {

        public TaskOutcome {
            requiredText("taskId", taskId);
            state = Objects.requireNonNull(state, "state");
            suggestions = suggestions == null ? List.of() : List.copyOf(suggestions);
            reason = reason == null ? "" : reason;
            if (reason.chars().anyMatch(Character::isISOControl)) {
                throw new IllegalArgumentException("reason must be control-free");
            }
            if (state != TaskTerminalState.PRIMARY_COMPLETED
                    && state != TaskTerminalState.FALLBACK_COMPLETED
                    && !suggestions.isEmpty()) {
                throw new IllegalArgumentException("non-completed task cannot carry suggestions");
            }
        }
    }

    public record Result(
            Optional<String> inputSnapshotDigest,
            OverallState state,
            int planned,
            int accounted,
            List<TaskOutcome> outcomes,
            Optional<AuthoringSuggestion> suggestion,
            int fallbackCount) {

        public Result {
            inputSnapshotDigest = Objects.requireNonNull(inputSnapshotDigest, "inputSnapshotDigest");
            state = Objects.requireNonNull(state, "state");
            outcomes = List.copyOf(outcomes);
            suggestion = Objects.requireNonNull(suggestion, "suggestion");
            if (planned < 0 || accounted < 0 || planned != accounted || outcomes.size() != planned) {
                throw new IllegalArgumentException("planned/accounted coverage mismatch");
            }
            if (fallbackCount < 0 || fallbackCount > planned) {
                throw new IllegalArgumentException("invalid fallback count");
            }
            Set<String> ids = new LinkedHashSet<>();
            outcomes.forEach(outcome -> {
                if (!ids.add(outcome.taskId())) {
                    throw new IllegalArgumentException("duplicate task outcome: " + outcome.taskId());
                }
            });
            if (state == OverallState.REJECTED && suggestion.isPresent()) {
                throw new IllegalArgumentException("rejected request cannot carry a suggestion");
            }
        }

        static Result rejected(String reason) {
            return new Result(
                    Optional.empty(), OverallState.REJECTED, 0, 0, List.of(), Optional.empty(), 0);
        }
    }

    /** Run the frozen request through primary groups and deterministic fallbacks. */
    public Result run(Request request, GroupExecutor primary, FallbackExecutor fallback) {
        Objects.requireNonNull(request, "request");
        Objects.requireNonNull(primary, "primary");
        try {
            validateRequest(request);
        } catch (IllegalArgumentException e) {
            return Result.rejected(e.getMessage());
        }

        List<AuthoringTask> frozen = request.tasks().stream()
                .sorted(Comparator.comparing(AuthoringTask::taskId))
                .toList();
        String inputDigest = digestRequest(request, frozen);
        List<TaskOutcome> outcomes = new ArrayList<>();
        int fallbackCount = 0;
        for (int start = 0; start < frozen.size(); start += request.groupSize()) {
            List<AuthoringTask> group = frozen.subList(start,
                    Math.min(start + request.groupSize(), frozen.size()));
            Execution groupExecution = safeGroupExecute(primary, group);
            boolean completeGroup = groupExecution.state() == AttemptState.SUCCESS
                    && hasExactlyTaskIds(groupExecution, group)
                    && group.stream().allMatch(task ->
                            !groupExecution.suggestions().get(task.taskId()).isEmpty());
            if (completeGroup) {
                for (AuthoringTask task : group) {
                    List<Suggestion> suggestions = groupExecution.suggestions().get(task.taskId());
                    if (suggestions.isEmpty()) {
                        outcomes.add(new TaskOutcome(task.taskId(), TaskTerminalState.EMPTY,
                                List.of(), "primary capability returned no suggestions"));
                    } else {
                        outcomes.add(validatedOutcome(request, task, TaskTerminalState.PRIMARY_COMPLETED,
                                suggestions, groupExecution.reason()));
                    }
                }
                continue;
            }

            for (AuthoringTask task : group) {
                fallbackCount++;
                Execution fallbackExecution = safeFallbackExecute(fallback, task);
                outcomes.add(toFallbackOutcome(request, task, fallbackExecution, groupExecution.reason()));
            }
        }

        outcomes.sort(Comparator.comparing(TaskOutcome::taskId));
        List<Suggestion> allSuggestions = outcomes.stream()
                .flatMap(outcome -> outcome.suggestions().stream())
                .toList();
        Optional<AuthoringSuggestion> suggestion = allSuggestions.isEmpty()
                ? Optional.empty()
                : Optional.of(new AuthoringSuggestion(
                        AuthoringSuggestion.KIND,
                        AuthoringSuggestion.SCHEMA_VERSION,
                        request.source(),
                        request.scope(),
                        allSuggestions,
                        AuthoringSuggestion.Trust.untrusted()));
        if (suggestion.isPresent()) {
            try {
                suggestion.get().validateSourceRefs(request.permittedSourceRefs());
            } catch (IllegalArgumentException e) {
                // Capability output is untrusted. Remove only the offending
                // task's output while preserving its accounted terminal state.
                List<TaskOutcome> sanitized = new ArrayList<>();
                for (TaskOutcome outcome : outcomes) {
                    if (outcome.suggestions().stream().allMatch(value ->
                            request.permittedSourceRefs().containsAll(value.sourceRefs()))) {
                        sanitized.add(outcome);
                    } else {
                        sanitized.add(new TaskOutcome(outcome.taskId(), TaskTerminalState.FAILED,
                                List.of(), "capability returned a source reference outside the caller ACL"));
                    }
                }
                outcomes = sanitized;
                allSuggestions = outcomes.stream()
                        .flatMap(outcome -> outcome.suggestions().stream())
                        .toList();
                suggestion = allSuggestions.isEmpty()
                        ? Optional.empty()
                        : Optional.of(new AuthoringSuggestion(
                                AuthoringSuggestion.KIND,
                                AuthoringSuggestion.SCHEMA_VERSION,
                                request.source(),
                                request.scope(),
                                allSuggestions,
                                AuthoringSuggestion.Trust.untrusted()));
            }
        }

        OverallState overall = overallState(outcomes, fallbackCount);
        return new Result(Optional.of(inputDigest), overall, frozen.size(), outcomes.size(), outcomes,
                suggestion, fallbackCount);
    }

    private static void validateRequest(Request request) {
        if (!CAPABILITY.equals(request.source().capability())) {
            throw new IllegalArgumentException("orchestrator source capability mismatch");
        }
        for (AuthoringTask task : request.tasks()) {
            if (!request.scope().standardIds().contains(task.standardId())) {
                throw new IllegalArgumentException("task standard is outside the frozen scope: " + task.taskId());
            }
            if (!request.permittedSourceRefs().containsAll(task.inputRefs())) {
                throw new IllegalArgumentException("task input reference is outside the caller ACL: " + task.taskId());
            }
            if (task.type() == null) {
                throw new IllegalArgumentException("task type is missing: " + task.taskId());
            }
        }
        Set<String> ids = new LinkedHashSet<>();
        for (AuthoringTask task : request.tasks()) {
            if (!ids.add(task.taskId())) {
                throw new IllegalArgumentException("duplicate task id: " + task.taskId());
            }
        }
    }

    private static String digestRequest(Request request, List<AuthoringTask> frozen) {
        StringBuilder canonical = new StringBuilder();
        canonical.append(request.source().sourceSnapshot()).append('\n')
                .append(request.source().configDigest()).append('\n')
                .append(request.scope().repositoryIdentity()).append('\n')
                .append(request.groupSize()).append('\n');
        for (AuthoringTask task : frozen) {
            canonical.append(task.taskId()).append('|')
                    .append(task.standardId()).append('|')
                    .append(task.type()).append('|')
                    .append(String.join(",", task.inputRefs())).append('\n');
        }
        return "sha256:" + HexFormat.of().formatHex(sha256(canonical.toString().getBytes(StandardCharsets.UTF_8)));
    }

    private static Execution safeGroupExecute(GroupExecutor executor, List<AuthoringTask> group) {
        try {
            Execution execution = executor.execute(List.copyOf(group));
            return Objects.requireNonNull(execution, "group execution");
        } catch (Exception e) {
            return Execution.failure(AttemptState.FAILED, "primary capability failed: " + safeMessage(e));
        }
    }

    private static Execution safeFallbackExecute(FallbackExecutor executor, AuthoringTask task) {
        if (executor == null) {
            return Execution.failure(AttemptState.FAILED, "no fallback capability configured");
        }
        try {
            Execution execution = executor.execute(task);
            return Objects.requireNonNull(execution, "fallback execution");
        } catch (Exception e) {
            return Execution.failure(AttemptState.FAILED, "fallback capability failed: " + safeMessage(e));
        }
    }

    private static boolean hasExactlyTaskIds(Execution execution, List<AuthoringTask> group) {
        Set<String> expected = group.stream().map(AuthoringTask::taskId).collect(java.util.stream.Collectors.toCollection(LinkedHashSet::new));
        return execution.suggestions().keySet().equals(expected);
    }

    private static TaskOutcome toFallbackOutcome(
            Request request, AuthoringTask task, Execution fallback, String primaryReason) {
        if (fallback.state() == AttemptState.SUCCESS
                && fallback.suggestions().keySet().equals(Set.of(task.taskId()))) {
            List<Suggestion> suggestions = fallback.suggestions().get(task.taskId());
            if (!suggestions.isEmpty()) {
                return validatedOutcome(request, task, TaskTerminalState.FALLBACK_COMPLETED,
                        suggestions, primaryReason);
            }
            return new TaskOutcome(task.taskId(), TaskTerminalState.EMPTY, List.of(),
                    "fallback capability returned no suggestions");
        }
        return new TaskOutcome(task.taskId(), toTerminalState(fallback.state()), List.of(),
                mergeReasons(primaryReason, fallback.reason()));
    }

    private static TaskOutcome validatedOutcome(
            Request request, AuthoringTask task, TaskTerminalState state,
            List<Suggestion> suggestions, String reason) {
        try {
            for (Suggestion suggestion : suggestions) {
                if (!request.permittedSourceRefs().containsAll(suggestion.sourceRefs())) {
                    throw new IllegalArgumentException("source reference outside ACL");
                }
            }
            return new TaskOutcome(task.taskId(), state, suggestions, reason);
        } catch (IllegalArgumentException e) {
            return new TaskOutcome(task.taskId(), TaskTerminalState.FAILED, List.of(),
                    "invalid capability suggestion: " + e.getMessage());
        }
    }

    private static TaskTerminalState toTerminalState(AttemptState state) {
        return switch (state) {
            case EMPTY -> TaskTerminalState.EMPTY;
            case TIMED_OUT -> TaskTerminalState.TIMED_OUT;
            case REJECTED -> TaskTerminalState.REJECTED;
            case FAILED, SUCCESS -> TaskTerminalState.FAILED;
        };
    }

    private static OverallState overallState(List<TaskOutcome> outcomes, int fallbackCount) {
        if (outcomes.stream().anyMatch(value -> value.state() == TaskTerminalState.FAILED)) {
            return OverallState.FAILED;
        }
        if (fallbackCount > 0 || outcomes.stream().anyMatch(value ->
                value.state() != TaskTerminalState.PRIMARY_COMPLETED)) {
            return OverallState.DEGRADED;
        }
        return OverallState.COMPLETED;
    }

    private static String mergeReasons(String first, String second) {
        if (first == null || first.isBlank()) return second == null ? "" : second;
        if (second == null || second.isBlank()) return first;
        return first + "; " + second;
    }

    private static String safeMessage(Exception e) {
        String message = e.getMessage();
        return message == null || message.isBlank() ? e.getClass().getSimpleName() : message;
    }

    private static byte[] sha256(byte[] bytes) {
        try {
            return MessageDigest.getInstance("SHA-256").digest(bytes);
        } catch (NoSuchAlgorithmException e) {
            throw new AssertionError("JDK must provide SHA-256", e);
        }
    }

    private static void requiredText(String name, String value) {
        if (value == null || value.isBlank() || value.chars().anyMatch(Character::isISOControl)) {
            throw new IllegalArgumentException(name + " must be non-empty and control-free");
        }
    }

    public AuthoringOrchestrator() {}
}
