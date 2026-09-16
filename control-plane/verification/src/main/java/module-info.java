/**
 * Verification Lab capability: runs Candidates against fixtures and produces
 * EvidenceBundle skeletons. It consumes factory outputs and never owns authoring
 * or compilation responsibilities (ADR-0018).
 */
module io.ruleops.verification {
  requires io.ruleops.authoring;
  requires transitive io.ruleops.factory;
  exports io.ruleops.verification;
}
