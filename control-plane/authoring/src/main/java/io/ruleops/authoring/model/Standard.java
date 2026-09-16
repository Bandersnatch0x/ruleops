package io.ruleops.authoring.model;

import com.fasterxml.jackson.annotation.JsonProperty;
import java.util.List;

/**
 * Governed Standard asset (spec §7): the organization's formal requirement for
 * code, with identity, version, owner and applicability. A Standard holds
 * Scenarios — the smallest business-semantic units with an independent machine
 * coverage expectation (CONTEXT: Standard, Scenario).
 */
public record Standard(
        String apiVersion,
        String kind,
        Metadata metadata,
        Spec spec) {

    public record Metadata(String id, String version, String owner) {}

    public record Spec(
            String title,
            String description,
            List<String> languages,
            List<Scenario> scenarios) {}

    /** A Scenario with its machine coverage expectation (spec §7). */
    public record Scenario(String id, String expected) {}
}
