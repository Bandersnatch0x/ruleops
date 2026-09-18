package io.ruleops.authoring.model;

import com.fasterxml.jackson.annotation.JsonCreator;
import com.fasterxml.jackson.annotation.JsonValue;
import java.util.Arrays;

/**
 * The seven closed Rule IR v1 node kinds (spec §10). Finite by design — no
 * universal DSL: complex interprocedural dataflow is expressed by dedicated
 * Backends/NativeRule, never squeezed into this IR.
 */
public enum RuleIrKind {
    EXISTING_RULE("ExistingRule"),
    STRUCTURAL_RULE("StructuralRule"),
    SYMBOL_RULE("SymbolRule"),
    TYPE_RULE("TypeRule"),
    BYTECODE_RULE("BytecodeRule"),
    ARCHITECTURE_RULE("ArchitectureRule"),
    NATIVE_RULE("NativeRule");

    private final String yamlName;

    RuleIrKind(String yamlName) {
        this.yamlName = yamlName;
    }

    /** The kind name as it appears in governed YAML documents. */
    @JsonValue
    public String yamlName() {
        return yamlName;
    }

    @JsonCreator
    public static RuleIrKind fromYamlName(String name) {
        return Arrays.stream(values())
                .filter(kind -> kind.yamlName.equals(name))
                .findFirst()
                .orElse(null);
    }
}
