package io.ruleops.authoring.model;

import java.util.List;

/**
 * Rule IR v1 (spec §10): a finite, typed, verifiable intermediate
 * representation. {@link RuleIrKind} closes the kind set. No universal DSL.
 *
 * <p>Deviation note: unlike the §10.1 prose example, governed Rule IR documents
 * carry {@code apiVersion} — every governed authoring asset is versioned so the
 * validate contract is uniform across Standard / RuleIntent / Rule IR.
 */
public record RuleIr(
        String apiVersion,
        RuleIrKind kind,
        String language,
        Match match,
        Scope scope,
        String onUnresolved) {

    public record Match(String node, Symbol symbol) {}

    /** Normalized symbol identity; owner/name are FQN dotted forms. */
    public record Symbol(String owner, String name, List<String> parameters) {}

    public record Scope(List<String> sourceSets) {}
}
