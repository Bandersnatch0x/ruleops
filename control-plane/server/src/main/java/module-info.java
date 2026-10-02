/** V0.2 RuleOps Server domain/API seam; authority remains outside this adapter. */
module io.ruleops.server {
  requires jdk.httpserver;
  requires java.net.http;
  requires com.fasterxml.jackson.core;
  requires com.fasterxml.jackson.databind;
  exports io.ruleops.server;
}
