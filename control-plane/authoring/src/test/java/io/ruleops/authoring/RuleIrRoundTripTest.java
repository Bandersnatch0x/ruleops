package io.ruleops.authoring;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.dataformat.yaml.YAMLFactory;
import io.ruleops.authoring.model.RuleIr;
import io.ruleops.authoring.model.RuleIrKind;
import java.util.List;
import org.junit.jupiter.api.Test;

/** Rule IR v1 serialization must round-trip losslessly for all seven kinds. */
class RuleIrRoundTripTest {

    private static final ObjectMapper YAML = new ObjectMapper(new YAMLFactory());

    private static RuleIr irOfKind(RuleIrKind kind) {
        return new RuleIr(
                "rulefactory.io/v1",
                kind,
                "java",
                new RuleIr.Match(
                        "method-call",
                        new RuleIr.Symbol("java.lang.System", "exit", List.of("int"))),
                new RuleIr.Scope(List.of("main")),
                "unknown");
    }

    @Test
    void everyIrKindRoundTrips() throws Exception {
        for (RuleIrKind kind : RuleIrKind.values()) {
            RuleIr original = irOfKind(kind);
            String yaml = YAML.writeValueAsString(original);
            RuleIr reparsed = YAML.readValue(yaml, RuleIr.class);
            assertEquals(original, reparsed, () -> "round-trip failed for " + kind);
        }
    }

    @Test
    void kindSerializesToItsGovernedYamlName() throws Exception {
        String yaml = YAML.writeValueAsString(irOfKind(RuleIrKind.SYMBOL_RULE));
        assertTrue(yaml.contains("SymbolRule"), () -> "expected governed name, got: " + yaml);
    }
}
