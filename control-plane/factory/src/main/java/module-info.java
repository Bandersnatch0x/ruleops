/**
 * Factory Kernel capability: compiles governed authoring assets into
 * verification-ready Candidates. It consumes the authoring capability's owned
 * formats and is consumed by verification (ADR-0018).
 */
module io.ruleops.factory {
  requires transitive io.ruleops.authoring;
  requires com.fasterxml.jackson.databind;
  requires com.fasterxml.jackson.dataformat.yaml;
  exports io.ruleops.factory;
  exports io.ruleops.factory.candidate;
  exports io.ruleops.factory.pmd;
  exports io.ruleops.factory.symbol;
  opens io.ruleops.factory.candidate to com.fasterxml.jackson.databind;
}
