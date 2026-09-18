package io.ruleops.authoring.model;

import com.fasterxml.jackson.annotation.JsonProperty;
import java.util.List;

/**
 * Governed RuleIntent asset (spec §9): the business semantics, scope and
 * required analysis capabilities a Scenario should be checked with, free of any
 * Backend implementation. The schema is a closed field set and defines no
 * executable-script field, so a RuleIntent cannot embed one (spec: RuleIntent
 * 禁止嵌入任意可执行脚本).
 */
public record RuleIntent(
        String apiVersion,
        String kind,
        Metadata metadata,
        Spec spec) {

    public record Metadata(String id, String version) {}

    public record Spec(
            StandardRef standard,
            Language language,
            Intent intent,
            Scope scope,
            Semantics semantics,
            List<String> requiredCapabilities,
            Unresolved unresolved,
            Examples examples,
            Severity severity) {}

    /** The Standard + Scenario this intent makes checkable. */
    public record StandardRef(String id, String version, String scenario) {}

    public record Language(String id, List<String> sourceLevels) {}

    public record Intent(String statement) {}

    public record Scope(List<String> includeSourceSets, List<String> excludeSourceSets) {}

    public record Semantics(Target target) {}

    /** Typed semantic target; values are a closed set validated by AuthoringValidator. */
    public record Target(
            String kind,
            String declaringType,
            String method,
            String invocation,
            List<String> parameters) {}

    public record Unresolved(String behavior) {}

    /** Example snippets used later as verification fixtures; data, never executed. */
    public record Examples(
            List<String> bad,
            List<String> good,
            List<String> holdout,
            List<String> nearMiss) {

        /** Source-compatible constructor for pre-Phase-2 two-list callers. */
        public Examples(List<String> bad, List<String> good) {
            this(bad, good, List.of(), List.of());
        }
    }

    /** Default gate severity; YAML key is `default` (a Java reserved word). */
    public record Severity(@JsonProperty("default") String defaultLevel) {}
}
