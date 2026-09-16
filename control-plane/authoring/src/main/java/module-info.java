/**
 * Authoring capability: owned formats and validation for Standard/Scenario,
 * RuleIntent and Rule IR assets. Implementation arrives with Phase 1 ticket #4;
 * this seam fixes the module boundary first (ADR-0018).
 */
module io.ruleops.authoring {
  requires com.fasterxml.jackson.annotation;
  requires transitive com.fasterxml.jackson.core;
  requires transitive com.fasterxml.jackson.databind;
  requires transitive com.fasterxml.jackson.dataformat.yaml;
  exports io.ruleops.authoring;
  exports io.ruleops.authoring.model;
  // YAML binding needs reflective access to the record canonical constructors.
  opens io.ruleops.authoring to com.fasterxml.jackson.databind;
  opens io.ruleops.authoring.model to com.fasterxml.jackson.databind;
}
