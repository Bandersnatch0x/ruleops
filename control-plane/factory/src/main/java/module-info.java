/**
 * Factory Kernel capability: compiles governed authoring assets into
 * verification-ready Candidates. It consumes the authoring capability's owned
 * formats and is consumed by verification (ADR-0018).
 */
module io.ruleops.factory {
  requires transitive io.ruleops.authoring;
  exports io.ruleops.factory;
}
