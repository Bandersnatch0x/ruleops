package io.ruleops.authoring.model;

import java.util.List;

/**
 * Rule IR v1 (spec §10): a finite, typed, verifiable intermediate
 * representation. The kind IS the IR node type — one of the seven closed kinds
 * (ExistingRule, StructuralRule, SymbolRule, TypeRule, BytecodeRule,
 * ArchitectureRule, NativeRule). No universal DSL; complex interprocedural
 * dataflow is expressed by dedicated Backends/NativeRule, never squeezed into
 * this IR.
 */
public record RuleIr(
        String apiVersion,
        String kind,
        String language,
        Match match,
        Scope scope,
        String onUnresolved) {

    public record Match(String node, Symbol symbol) {}

    /** Normalized symbol identity; owner/name are FQN dotted forms. */
    public record Symbol(String owner, String name, List<String> parameters) {}

    public record Scope(List<String> sourceSets) {}
}
