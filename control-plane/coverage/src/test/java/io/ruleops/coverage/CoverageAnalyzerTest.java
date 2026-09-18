package io.ruleops.coverage;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import io.ruleops.authoring.model.Standard;
import java.util.List;
import org.junit.jupiter.api.Test;

class CoverageAnalyzerTest {
    @Test
    void usesEveryStandardScenarioAsFixedDenominator() throws Exception {
        var standard = standard();
        var inventory = new CoverageInventory(List.of(
                new ScenarioClassification("runtime-halt", CoverageCategory.STANDARD_AMBIGUOUS, ""),
                new ScenarioClassification("direct-system-exit", CoverageCategory.EXISTING_RULE, "corp.java.no-system-exit"),
                new ScenarioClassification("reflective-invocation", CoverageCategory.NOT_MACHINE_DETECTABLE, "")));

        var ledger = new CoverageAnalyzer().analyze(standard, inventory);
        assertEquals(List.of("direct-system-exit", "reflective-invocation", "runtime-halt"),
                ledger.entries().stream().map(CoverageEntry::scenarioId).toList());
        assertEquals(CoverageState.COVERED, ledger.entries().get(0).state());
        assertEquals(CoverageState.BLOCKED, ledger.entries().get(2).state());
    }

    @Test
    void rejectsMissingUnknownDuplicateAndInvalidRuleBindings() {
        var analyzer = new CoverageAnalyzer();
        var standard = standard();
        assertEquals("CLASSIFICATION_MISSING", assertThrows(CoverageException.class,
                () -> analyzer.analyze(standard, new CoverageInventory(List.of()))).code());
        assertEquals("SCENARIO_UNKNOWN", assertThrows(CoverageException.class,
                () -> analyzer.analyze(standard, new CoverageInventory(List.of(
                        new ScenarioClassification("other", CoverageCategory.NEW_RULE_REQUIRED, ""))))).code());
        assertEquals("RULE_ID_DUPLICATE", assertThrows(CoverageException.class,
                () -> analyzer.analyze(standard, new CoverageInventory(List.of(
                        new ScenarioClassification("direct-system-exit", CoverageCategory.EXISTING_RULE, "same"),
                        new ScenarioClassification("runtime-halt", CoverageCategory.EXISTING_RULE_NOT_ENABLED, "same"),
                        new ScenarioClassification("reflective-invocation", CoverageCategory.NOT_MACHINE_DETECTABLE, ""))))).code());
        assertEquals("RULE_BINDING_INVALID", assertThrows(CoverageException.class,
                () -> analyzer.analyze(standard, new CoverageInventory(List.of(
                        new ScenarioClassification("direct-system-exit", CoverageCategory.NEW_RULE_REQUIRED, "old-rule"),
                        new ScenarioClassification("runtime-halt", CoverageCategory.STANDARD_AMBIGUOUS, ""),
                        new ScenarioClassification("reflective-invocation", CoverageCategory.NOT_MACHINE_DETECTABLE, ""))))).code());
    }

    @Test
    void emitsDeterministicDurableYamlAndDigest() throws Exception {
        var inventory = new CoverageInventory(List.of(
                new ScenarioClassification("reflective-invocation", CoverageCategory.NOT_MACHINE_DETECTABLE, ""),
                new ScenarioClassification("direct-system-exit", CoverageCategory.EXISTING_RULE, "corp.java.no-system-exit"),
                new ScenarioClassification("runtime-halt", CoverageCategory.STANDARD_AMBIGUOUS, "")));
        var analyzer = new CoverageAnalyzer();
        var first = analyzer.analyze(standard(), inventory);
        var second = analyzer.analyze(standard(), inventory);
        assertEquals(first.asYaml(), second.asYaml());
        assertEquals(first.digest(), second.digest());
        assertEquals("apiVersion: rulefactory.io/v1\nkind: CoverageLedger\nstandard:\n"
                + "  id: SEC-JAVA-017\n  version: 1.0.0\ncoverage:\n"
                + "  direct-system-exit:\n    category: EXISTING_RULE\n    status: covered\n    rule: corp.java.no-system-exit\n"
                + "  reflective-invocation:\n    category: NOT_MACHINE_DETECTABLE\n    status: unsupported\n    rule: null\n"
                + "  runtime-halt:\n    category: STANDARD_AMBIGUOUS\n    status: blocked\n    rule: null\n", first.asYaml());
    }

    private static Standard standard() {
        return new Standard("rulefactory.io/v1", "Standard",
                new Standard.Metadata("SEC-JAVA-017", "1.0.0", "security-platform"),
                new Standard.Spec("title", "description", List.of("java"), List.of(
                        new Standard.Scenario("direct-system-exit", "detectable"),
                        new Standard.Scenario("runtime-halt", "needs-clarification"),
                        new Standard.Scenario("reflective-invocation", "unsupported"))));
    }
}
