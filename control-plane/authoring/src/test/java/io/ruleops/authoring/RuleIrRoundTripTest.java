package io.ruleops.authoring;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.dataformat.yaml.YAMLFactory;
import io.ruleops.authoring.model.RuleIr;
import java.util.List;
import org.junit.jupiter.api.Test;

/** Rule IR v1 serialization must round-trip losslessly (typed closed model). */
class RuleIrRoundTripTest {

    private static final ObjectMapper YAML = new ObjectMapper(new YAMLFactory());

    @Test
    void symbolRuleRoundTrips() throws Exception {
        RuleIr original = new RuleIr(
                "rulefactory.io/v1",
                "SymbolRule",
                "java",
                new RuleIr.Match(
                        "method-call",
                        new RuleIr.Symbol("java.lang.System", "exit", List.of("int"))),
                new RuleIr.Scope(List.of("main")),
                "unknown");
        String yaml = YAML.writeValueAsString(original);
        RuleIr reparsed = YAML.readValue(yaml, RuleIr.class);
        assertEquals(original, reparsed);
    }
}
