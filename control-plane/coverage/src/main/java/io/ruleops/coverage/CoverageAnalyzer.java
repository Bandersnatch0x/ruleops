package io.ruleops.coverage;

import io.ruleops.authoring.model.Standard;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/** Builds a fixed-denominator Coverage Ledger without guessing omissions. */
public final class CoverageAnalyzer {
    public CoverageLedger analyze(Standard standard, CoverageInventory inventory) throws CoverageException {
        if (standard == null || standard.metadata() == null || standard.spec() == null
                || standard.metadata().id() == null || standard.metadata().version() == null) {
            throw new CoverageException("STANDARD_INVALID", "standard", "standard identity and spec are required");
        }
        if (standard.spec().scenarios() == null || standard.spec().scenarios().isEmpty()) {
            throw new CoverageException("SCENARIO_DENOMINATOR_EMPTY", "standard.spec.scenarios", "at least one scenario is required");
        }

        Map<String, Standard.Scenario> scenarios = new HashMap<>();
        for (int i = 0; i < standard.spec().scenarios().size(); i++) {
            var scenario = standard.spec().scenarios().get(i);
            if (scenario == null || scenario.id() == null || scenario.id().isBlank()) {
                throw new CoverageException("SCENARIO_INVALID", "standard.spec.scenarios[" + i + "]", "scenario id is required");
            }
            if (scenarios.put(scenario.id(), scenario) != null) {
                throw new CoverageException("SCENARIO_DUPLICATE", "standard.spec.scenarios[" + i + "]", "duplicate scenario: " + scenario.id());
            }
        }

        Map<String, ScenarioClassification> byScenario = new HashMap<>();
        Set<String> ruleIds = new HashSet<>();
        for (int i = 0; i < inventory.classifications().size(); i++) {
            var classification = inventory.classifications().get(i);
            if (classification == null) {
                throw new CoverageException("CLASSIFICATION_INVALID", "inventory[" + i + "]", "classification is required");
            }
            if (!scenarios.containsKey(classification.scenarioId())) {
                throw new CoverageException("SCENARIO_UNKNOWN", "inventory[" + i + "]", "unknown scenario: " + classification.scenarioId());
            }
            if (byScenario.put(classification.scenarioId(), classification) != null) {
                throw new CoverageException("CLASSIFICATION_DUPLICATE", "inventory[" + i + "]", "scenario classified more than once: " + classification.scenarioId());
            }
            validateRuleBinding(classification, i, ruleIds);
        }
        for (String scenarioId : scenarios.keySet()) {
            if (!byScenario.containsKey(scenarioId)) {
                throw new CoverageException("CLASSIFICATION_MISSING", "inventory", "no classification for scenario: " + scenarioId);
            }
        }

        List<CoverageEntry> entries = new ArrayList<>();
        for (var scenario : standard.spec().scenarios()) {
            var classification = byScenario.get(scenario.id());
            entries.add(new CoverageEntry(classification.scenarioId(), classification.category(), classification.ruleId()));
        }
        return new CoverageLedger(standard.metadata().id(), standard.metadata().version(), entries);
    }

    private static void validateRuleBinding(ScenarioClassification classification, int index, Set<String> ruleIds)
            throws CoverageException {
        boolean present = !classification.ruleId().isBlank();
        if (classification.category().requiresRule() && !present) {
            throw new CoverageException("RULE_ID_MISSING", "inventory[" + index + "].ruleId", "category requires a rule identity");
        }
        if (!classification.category().requiresRule() && present) {
            throw new CoverageException("RULE_BINDING_INVALID", "inventory[" + index + "].ruleId", "category cannot bind an existing rule identity");
        }
        if (present && !ruleIds.add(classification.ruleId())) {
            throw new CoverageException("RULE_ID_DUPLICATE", "inventory[" + index + "].ruleId", "rule identity appears more than once: " + classification.ruleId());
        }
    }
}
