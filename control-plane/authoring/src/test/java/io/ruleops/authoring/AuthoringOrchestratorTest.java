package io.ruleops.authoring;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import io.ruleops.authoring.orchestration.AuthoringOrchestrator;
import io.ruleops.authoring.orchestration.AuthoringOrchestrator.AttemptState;
import io.ruleops.authoring.orchestration.AuthoringOrchestrator.AuthoringTask;
import io.ruleops.authoring.orchestration.AuthoringOrchestrator.Execution;
import io.ruleops.authoring.orchestration.AuthoringOrchestrator.OverallState;
import io.ruleops.authoring.orchestration.AuthoringOrchestrator.Request;
import io.ruleops.authoring.orchestration.AuthoringOrchestrator.TaskTerminalState;
import io.ruleops.authoring.orchestration.AuthoringOrchestrator.TaskType;
import io.ruleops.authoring.suggestion.AuthoringSuggestion;
import io.ruleops.authoring.suggestion.AuthoringSuggestion.Suggestion;
import io.ruleops.authoring.suggestion.AuthoringSuggestion.SuggestionType;
import java.util.List;
import java.util.Map;
import java.util.Set;
import org.junit.jupiter.api.Test;

/** Contract tests for deterministic grouping, fallback and untrusted output. */
class AuthoringOrchestratorTest {

    private static String digest(char value) {
        return "sha256:" + String.valueOf(value).repeat(64);
    }

    private static Request request() {
        return new Request(
                new AuthoringSuggestion.Source(
                        AuthoringOrchestrator.CAPABILITY,
                        AuthoringOrchestrator.CAPABILITY_VERSION,
                        digest('a'),
                        digest('b'),
                        null,
                        null),
                new AuthoringSuggestion.Scope(List.of("SEC-JAVA-017"), digest('c'), "approved-metadata"),
                List.of(
                        new AuthoringTask("task-z", "SEC-JAVA-017", TaskType.NEAR_MISS, List.of("standard")),
                        new AuthoringTask("task-a", "SEC-JAVA-017", TaskType.SCENARIO, List.of("standard"))),
                Set.of("standard", "asset/task-a", "asset/task-z"),
                2);
    }

    private static Suggestion suggestion(String taskId, SuggestionType type) {
        return new Suggestion(type, "suggestion for " + taskId, List.of("asset/" + taskId), null);
    }

    @Test
    void successfulGroupsAreSortedAndRepeatable() {
        AuthoringOrchestrator orchestrator = new AuthoringOrchestrator();
        AuthoringOrchestrator.GroupExecutor primary = tasks -> new Execution(
                AttemptState.SUCCESS,
                Map.of(
                        "task-z", List.of(suggestion("task-z", SuggestionType.NEAR_MISS)),
                        "task-a", List.of(suggestion("task-a", SuggestionType.SCENARIO))),
                "primary");

        var first = orchestrator.run(request(), primary, task -> {
            throw new AssertionError("fallback must not run");
        });
        var second = orchestrator.run(request(), primary, task -> {
            throw new AssertionError("fallback must not run");
        });

        assertEquals(OverallState.COMPLETED, first.state());
        assertEquals(List.of("task-a", "task-z"), first.outcomes().stream()
                .map(AuthoringOrchestrator.TaskOutcome::taskId).toList());
        assertEquals(first.inputSnapshotDigest(), second.inputSnapshotDigest());
        assertEquals(first.suggestion().orElseThrow().digest(), second.suggestion().orElseThrow().digest());
        assertEquals(0, first.fallbackCount());
        assertFalse(first.suggestion().orElseThrow().trust().authoritative());
        assertFalse(first.suggestion().orElseThrow().trust().usableForGate());
    }

    @Test
    void failedGroupFallsBackWithoutLosingPlannedCoverage() {
        AuthoringOrchestrator orchestrator = new AuthoringOrchestrator();
        var result = orchestrator.run(
                request(),
                tasks -> Execution.failure(AttemptState.FAILED, "batch failed"),
                task -> Execution.success(task.taskId(), List.of(suggestion(task.taskId(), SuggestionType.POSITIVE))));

        assertEquals(OverallState.DEGRADED, result.state());
        assertEquals(2, result.planned());
        assertEquals(2, result.accounted());
        assertEquals(2, result.fallbackCount());
        assertEquals(List.of(TaskTerminalState.FALLBACK_COMPLETED, TaskTerminalState.FALLBACK_COMPLETED),
                result.outcomes().stream().map(AuthoringOrchestrator.TaskOutcome::state).toList());
        assertTrue(result.suggestion().isPresent());
    }

    @Test
    void timeoutEmptyAndRejectionRemainExplicitTerminalStates() {
        AuthoringOrchestrator orchestrator = new AuthoringOrchestrator();
        var result = orchestrator.run(
                request(),
                tasks -> Execution.failure(AttemptState.TIMED_OUT, "group timeout"),
                task -> task.taskId().equals("task-a")
                        ? Execution.failure(AttemptState.EMPTY, "no result")
                        : Execution.failure(AttemptState.REJECTED, "capability rejected"));

        assertEquals(OverallState.DEGRADED, result.state());
        assertEquals(List.of(TaskTerminalState.EMPTY, TaskTerminalState.REJECTED),
                result.outcomes().stream().map(AuthoringOrchestrator.TaskOutcome::state).toList());
        assertTrue(result.suggestion().isEmpty());
    }

    @Test
    void invalidCapabilityScopeIsRejectedWithoutSuggestion() {
        Request invalid = new Request(
                new AuthoringSuggestion.Source(
                        "wrong-capability", "0.1.0", digest('a'), digest('b'), null, null),
                request().scope(), request().tasks(), request().permittedSourceRefs(), 1);
        var result = new AuthoringOrchestrator().run(
                invalid,
                tasks -> Execution.success("task-a", List.of(suggestion("task-a", SuggestionType.SCENARIO))),
                task -> Execution.success(task.taskId(), List.of(suggestion(task.taskId(), SuggestionType.SCENARIO))));

        assertEquals(OverallState.REJECTED, result.state());
        assertTrue(result.suggestion().isEmpty());
        assertTrue(result.inputSnapshotDigest().isEmpty());
    }

    @Test
    void capabilitySourceReferenceOutsideAclIsDiscarded() {
        var result = new AuthoringOrchestrator().run(
                request(),
                tasks -> new Execution(AttemptState.SUCCESS, Map.of(
                        "task-a", List.of(new Suggestion(SuggestionType.SCENARIO, "secret", List.of("secret"), null)),
                        "task-z", List.of(suggestion("task-z", SuggestionType.NEAR_MISS))), "primary"),
                task -> Execution.failure(AttemptState.FAILED, "fallback unavailable"));

        assertEquals(OverallState.FAILED, result.state());
        assertEquals(TaskTerminalState.FAILED, result.outcomes().get(0).state());
        assertTrue(result.outcomes().get(1).suggestions().size() == 1);
        assertTrue(result.suggestion().isPresent());
    }
}
