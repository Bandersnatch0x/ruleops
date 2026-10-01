package io.ruleops.server;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

class RuleOpsHttpServerTest {
    private static final ObjectMapper JSON = new ObjectMapper();
    private final HttpClient client = HttpClient.newHttpClient();
    private RuleOpsHttpServer server;

    @BeforeEach
    void start() throws Exception {
        server = RuleOpsHttpServer.loopback(0, new RuleOpsStore());
        server.start();
    }

    @AfterEach
    void stop() {
        server.close();
    }

    @Test
    void healthAndResourceLifecycleUseVersionedJson() throws Exception {
        HttpResponse<String> health = get("/health");
        assertEquals(200, health.statusCode());
        assertEquals("rulefactory.io/v1", JSON.readTree(health.body()).get("apiVersion").asText());

        HttpResponse<String> created = post("/rules", "rule-1", "1.0.0",
                "{\"kind\":\"RuleIntent\",\"title\":\"safe\"}", "op-create");
        assertEquals(201, created.statusCode());
        JsonNode resource = JSON.readTree(created.body()).get("resource");
        assertEquals("CANDIDATE", resource.get("state").asText());
        assertTrue(resource.get("digest").asText().startsWith("sha256:"));

        HttpResponse<String> replay = post("/rules", "rule-1", "1.0.0",
                "{\"title\":\"safe\",\"kind\":\"RuleIntent\"}", "op-create");
        assertEquals(200, replay.statusCode());
        assertTrue(JSON.readTree(replay.body()).get("replayed").asBoolean());

        HttpResponse<String> fetched = get("/rules/rule-1");
        assertEquals(200, fetched.statusCode());
        assertEquals(resource.get("digest").asText(), JSON.readTree(fetched.body()).get("resource").get("digest").asText());

        HttpResponse<String> audit = get("/audit");
        assertEquals(200, audit.statusCode());
        assertEquals(1, JSON.readTree(audit.body()).get("events").size());
    }

    @Test
    void auditEndpointPaginatesWithAStableSequenceCursor() throws Exception {
        for (int i = 1; i <= 3; i++) {
            assertEquals(201, post("/rules", "audit-rule-" + i, "1.0.0",
                    "{\"kind\":\"RuleIntent\",\"index\":" + i + "}", "audit-create-" + i).statusCode());
        }

        JsonNode first = JSON.readTree(get("/audit?limit=2").body());
        assertEquals(2, first.get("events").size());
        assertTrue(first.get("hasMore").asBoolean());
        assertEquals(2, first.get("nextAfterSequence").asLong());

        JsonNode second = JSON.readTree(get("/audit?afterSequence=2&limit=2").body());
        assertEquals(1, second.get("events").size());
        assertEquals(3, second.get("events").get(0).get("sequence").asInt());
        assertTrue(!second.get("hasMore").asBoolean());

        HttpResponse<String> invalid = get("/audit?limit=0");
        assertEquals(400, invalid.statusCode());
        assertEquals("AUDIT_QUERY_INVALID", JSON.readTree(invalid.body()).get("code").asText());
    }

    @Test
    void conflictingIdempotencyAndImmutableIdentityFailClosed() throws Exception {
        assertEquals(201, post("/rules", "rule-1", "1.0.0", "{\"kind\":\"RuleIntent\"}", "op-1").statusCode());
        HttpResponse<String> idemConflict = post("/rules", "rule-2", "1.0.0", "{\"kind\":\"RuleIntent\"}", "op-1");
        assertEquals(409, idemConflict.statusCode());
        assertEquals("IDEMPOTENCY_CONFLICT", JSON.readTree(idemConflict.body()).get("code").asText());

        HttpResponse<String> immutableConflict = post("/rules", "rule-1", "1.0.0", "{\"kind\":\"Other\"}", "op-2");
        assertEquals(409, immutableConflict.statusCode());
        assertEquals("IMMUTABLE_CONFLICT", JSON.readTree(immutableConflict.body()).get("code").asText());
    }

    @Test
    void releasePromotionAndRevokeAreAuditedTransitions() throws Exception {
        assertEquals(201, post("/releases", "release-1", "1.0.0", "{\"ruleDigest\":\"sha256:x\"}", "release-create").statusCode());
        assertEquals(200, transition("/releases/release-1/promote", "1.0.0", "release-promote").statusCode());
        assertEquals("ACTIVE", JSON.readTree(get("/releases/release-1").body()).get("resource").get("state").asText());
        assertEquals(200, transition("/releases/release-1/revoke", "1.0.0", "release-revoke").statusCode());
        assertEquals("REVOKED", JSON.readTree(get("/releases/release-1").body()).get("resource").get("state").asText());
        assertEquals(3, JSON.readTree(get("/audit").body()).get("events").size());
    }

    @Test
    void lifecycleTransitionRequiresAnExplicitVersionAndTargetsOnlyThatImmutableRevision() throws Exception {
        assertEquals(201, post("/releases", "release-shared", "1.0.0",
                "{\"ruleDigest\":\"sha256:v1\"}", "release-v1").statusCode());
        assertEquals(201, post("/releases", "release-shared", "2.0.0",
                "{\"ruleDigest\":\"sha256:v2\"}", "release-v2").statusCode());

        HttpResponse<String> missingVersion = transition("/releases/release-shared/promote", null,
                "release-missing-version");
        assertEquals(409, missingVersion.statusCode());
        assertEquals("RESOURCE_VERSION_REQUIRED", JSON.readTree(missingVersion.body()).get("code").asText());

        assertEquals(200, transition("/releases/release-shared/promote", "2.0.0",
                "release-v2-promote").statusCode());
        assertEquals("CANDIDATE", JSON.readTree(get("/releases/release-shared?version=1.0.0").body())
                .get("resource").get("state").asText());
        assertEquals("ACTIVE", JSON.readTree(get("/releases/release-shared?version=2.0.0").body())
                .get("resource").get("state").asText());

        HttpResponse<String> unknownVersion = transition("/releases/release-shared/revoke", "9.0.0",
                "release-unknown-version");
        assertEquals(404, unknownVersion.statusCode());
        assertEquals("NOT_FOUND", JSON.readTree(unknownVersion.body()).get("code").asText());
    }

    @Test
    void lifecycleTransitionOperationIdCannotBeReusedAcrossVersions() throws Exception {
        assertEquals(201, post("/releases", "release-idempotency", "1.0.0",
                "{\"ruleDigest\":\"sha256:v1\"}", "idempotency-v1").statusCode());
        assertEquals(201, post("/releases", "release-idempotency", "2.0.0",
                "{\"ruleDigest\":\"sha256:v2\"}", "idempotency-v2").statusCode());

        assertEquals(200, transition("/releases/release-idempotency/promote", "1.0.0",
                "same-transition-operation").statusCode());
        HttpResponse<String> conflict = transition("/releases/release-idempotency/promote", "2.0.0",
                "same-transition-operation");
        assertEquals(409, conflict.statusCode());
        assertEquals("IDEMPOTENCY_CONFLICT", JSON.readTree(conflict.body()).get("code").asText());
        assertEquals("CANDIDATE", JSON.readTree(get("/releases/release-idempotency?version=2.0.0").body())
                .get("resource").get("state").asText());
    }

    @Test
    void lifecycleTransitionRejectsMalformedVersionRequestBody() throws Exception {
        assertEquals(201, post("/releases", "release-body", "1.0.0",
                "{\"ruleDigest\":\"sha256:x\"}", "release-body-create").statusCode());

        HttpResponse<String> nonTextVersion = transitionBody("/releases/release-body/promote",
                "{\"version\":1}", "release-non-text-version");
        assertEquals(400, nonTextVersion.statusCode());
        assertEquals("TRANSITION_BODY_INVALID", JSON.readTree(nonTextVersion.body()).get("code").asText());

        HttpResponse<String> extraField = transitionBody("/releases/release-body/promote",
                "{\"version\":\"1.0.0\",\"id\":\"release-body\"}", "release-extra-field");
        assertEquals(400, extraField.statusCode());
        assertEquals("TRANSITION_BODY_INVALID", JSON.readTree(extraField.body()).get("code").asText());

        HttpResponse<String> unknownField = transitionBody("/releases/release-body/promote",
                "{\"unexpected\":\"value\"}", "release-unknown-field");
        assertEquals(400, unknownField.statusCode());
        assertEquals("TRANSITION_BODY_INVALID", JSON.readTree(unknownField.body()).get("code").asText());
    }

    @Test
    void replayAfterALaterTransitionReturnsTheOriginalVersionQualifiedResult() throws Exception {
        assertEquals(201, post("/releases", "release-replay", "1.0.0",
                "{\"ruleDigest\":\"sha256:replay\"}", "release-replay-create").statusCode());
        assertEquals(200, transition("/releases/release-replay/promote", "1.0.0",
                "release-replay-promote").statusCode());
        assertEquals(200, transition("/releases/release-replay/revoke", "1.0.0",
                "release-replay-revoke").statusCode());

        HttpResponse<String> replay = transition("/releases/release-replay/promote", "1.0.0",
                "release-replay-promote");
        assertEquals(200, replay.statusCode());
        assertTrue(JSON.readTree(replay.body()).get("replayed").asBoolean());
        assertEquals("ACTIVE", JSON.readTree(replay.body()).get("resource").get("state").asText());
        assertEquals("REVOKED", JSON.readTree(get("/releases/release-replay?version=1.0.0").body())
                .get("resource").get("state").asText());
        assertEquals(3, JSON.readTree(get("/audit").body()).get("events").size());
    }

    @Test
    void privacyAndAuditHeadersAreRequired() throws Exception {
        HttpResponse<String> missingHeader = postWithoutHeaders("/feedback", "feedback-1", "1.0.0", "{\"label\":\"fp\"}");
        assertEquals(400, missingHeader.statusCode());
        assertEquals("HEADER_MISSING", JSON.readTree(missingHeader.body()).get("code").asText());

        HttpResponse<String> source = post("/feedback", "feedback-1", "1.0.0",
                "{\"sourceCode\":\"secret\"}", "feedback-1");
        assertEquals(400, source.statusCode());
        assertEquals("PRIVACY_FIELD_FORBIDDEN", JSON.readTree(source.body()).get("code").asText());

        HttpResponse<String> findings = get("/findings");
        assertEquals(200, findings.statusCode());
        assertEquals("digest-only", JSON.readTree(findings.body()).get("privacyProfile").asText());
    }

    @Test
    void thinClientGovernanceRequestsAreClosedDigestOnlyAndReplayable() throws Exception {
        ObjectNode feedbackPayload = JSON.createObjectNode();
        feedbackPayload.put("apiVersion", "rulefactory.io/v1");
        feedbackPayload.put("kind", "FeedbackDraft");
        feedbackPayload.put("schemaVersion", "feedback-draft-v1");
        feedbackPayload.put("status", "PROPOSED");
        feedbackPayload.put("findingDigest", "sha256:" + "b".repeat(64));
        feedbackPayload.put("findingFingerprint", "fp-1");
        feedbackPayload.put("ruleId", "corp.java.rule");
        feedbackPayload.put("classification", "true-fp");
        feedbackPayload.put("reason", "contract test");
        feedbackPayload.put("actor", "test-operator");
        String requestId = withEnvelopeIdentity(feedbackPayload);
        String feedback = feedbackPayload.toString();
        HttpResponse<String> accepted = post("/feedback", requestId, "feedback-draft-v1", feedback, requestId);
        assertEquals(201, accepted.statusCode());
        assertEquals("feedback", JSON.readTree(accepted.body()).get("resource").get("type").asText());
        assertEquals("OPEN", JSON.readTree(accepted.body()).get("resource").get("state").asText());

        HttpResponse<String> replay = post("/feedback", requestId, "feedback-draft-v1", feedback, requestId);
        assertEquals(200, replay.statusCode());
        assertTrue(JSON.readTree(replay.body()).get("replayed").asBoolean());

        ObjectNode exceptionPayload = JSON.createObjectNode();
        exceptionPayload.put("apiVersion", "rulefactory.io/v1");
        exceptionPayload.put("kind", "ExceptionRequest");
        exceptionPayload.put("schemaVersion", "exception-request-v1");
        exceptionPayload.put("status", "REQUESTED");
        exceptionPayload.put("findingDigest", "sha256:" + "e".repeat(64));
        exceptionPayload.put("findingFingerprint", "fp-2");
        exceptionPayload.put("ruleId", "corp.java.rule");
        exceptionPayload.put("scope", "src/**");
        exceptionPayload.put("reason", "contract test");
        exceptionPayload.put("actor", "test-operator");
        String exceptionId = withEnvelopeIdentity(exceptionPayload);
        String exception = exceptionPayload.toString();
        HttpResponse<String> exceptionAccepted = post("/exceptions", exceptionId, "exception-request-v1", exception, exceptionId);
        assertEquals(201, exceptionAccepted.statusCode());
        assertEquals("REQUESTED", JSON.readTree(exceptionAccepted.body()).get("resource").get("state").asText());

        String invalid = feedback.replace("PROPOSED", "APPROVED").replace(requestId, "sha256:" + "1".repeat(64));
        HttpResponse<String> invalidResponse = post("/feedback", "sha256:" + "1".repeat(64), "feedback-draft-v1", invalid,
                "sha256:" + "1".repeat(64));
        assertEquals(400, invalidResponse.statusCode());
        assertEquals("GOVERNANCE_REQUEST_INVALID", JSON.readTree(invalidResponse.body()).get("code").asText());
    }

    @Test
    void submissionStatusResolvesRequestOrArtifactWithoutReplayingWrites() throws Exception {
        ObjectNode feedbackPayload = JSON.createObjectNode();
        feedbackPayload.put("apiVersion", "rulefactory.io/v1");
        feedbackPayload.put("kind", "FeedbackDraft");
        feedbackPayload.put("schemaVersion", "feedback-draft-v1");
        feedbackPayload.put("status", "PROPOSED");
        feedbackPayload.put("findingDigest", "sha256:" + "1".repeat(64));
        feedbackPayload.put("findingFingerprint", "status-fp");
        feedbackPayload.put("ruleId", "corp.java.status");
        feedbackPayload.put("classification", "true-fp");
        feedbackPayload.put("reason", "status recovery test");
        feedbackPayload.put("actor", "status-operator");
        String requestId = withEnvelopeIdentity(feedbackPayload);
        String feedback = feedbackPayload.toString();
        String artifactDigest = JSON.readTree(feedback).get("digest").asText();

        HttpResponse<String> accepted = postGovernance("/feedback", requestId, "feedback-draft-v1", feedback,
                requestId, "status-operator", "status recovery test");
        assertEquals(201, accepted.statusCode());
        int auditAfterCreate = JSON.readTree(get("/audit").body()).get("events").size();

        HttpResponse<String> byRequest = get("/submissions/status?requestId=" + requestId);
        assertEquals(200, byRequest.statusCode());
        JsonNode requestStatus = JSON.readTree(byRequest.body());
        assertEquals("RECEIVED_ONLY", requestStatus.get("status").asText());
        assertEquals(requestId, requestStatus.get("requestId").asText());
        assertEquals(artifactDigest, requestStatus.get("artifactDigest").asText());
        assertEquals("feedback", requestStatus.get("resource").get("type").asText());
        assertEquals(requestId, requestStatus.get("resource").get("id").asText());
        assertTrue(!requestStatus.get("resource").has("payload"));
        assertEquals("status-operator", requestStatus.get("auditContext").get("actor").asText());
        assertEquals("status recovery test", requestStatus.get("auditContext").get("reason").asText());
        assertEquals("RECEIVED_ONLY", requestStatus.get("authority").asText());
        assertTrue(!requestStatus.get("automaticRetry").asBoolean());

        HttpResponse<String> byArtifact = get("/submissions/status?artifactDigest=" + artifactDigest);
        assertEquals(200, byArtifact.statusCode());
        assertEquals(requestId, JSON.readTree(byArtifact.body()).get("requestId").asText());
        assertEquals(artifactDigest, JSON.readTree(byArtifact.body()).get("artifactDigest").asText());

        HttpResponse<String> replay = postGovernance("/feedback", requestId, "feedback-draft-v1", feedback,
                requestId, "status-operator", "status recovery test");
        assertEquals(200, replay.statusCode());
        assertTrue(JSON.readTree(replay.body()).get("replayed").asBoolean());
        JsonNode replayStatus = JSON.readTree(get("/submissions/status?requestId=" + requestId).body());
        assertEquals("REPLAYED", replayStatus.get("status").asText());
        assertEquals("RECEIVED_ONLY", replayStatus.get("authority").asText());
        assertEquals(auditAfterCreate, JSON.readTree(get("/audit").body()).get("events").size());

        String unknownId = "sha256:" + "0".repeat(64);
        JsonNode unknown = JSON.readTree(get("/submissions/status?requestId=" + unknownId).body());
        assertEquals("UNKNOWN", unknown.get("status").asText());
        assertTrue(unknown.get("resource").isNull());
        assertTrue(unknown.get("auditContext").isNull());
        assertTrue(!unknown.get("automaticRetry").asBoolean());

        HttpResponse<String> conflict = get("/submissions/status?requestId=" + requestId
                + "&artifactDigest=" + "sha256:" + "2".repeat(64));
        assertEquals(409, conflict.statusCode());
        assertEquals("SUBMISSION_IDENTITY_CONFLICT", JSON.readTree(conflict.body()).get("code").asText());
        assertEquals(auditAfterCreate, JSON.readTree(get("/audit").body()).get("events").size());

        HttpResponse<String> duplicateSelector = get("/submissions/status?requestId=" + requestId
                + "&requestId=" + requestId);
        assertEquals(400, duplicateSelector.statusCode());
        assertEquals("SUBMISSION_SELECTOR_INVALID", JSON.readTree(duplicateSelector.body()).get("code").asText());
        HttpResponse<String> invalidSelector = get("/submissions/status?requestId=%25ZZ");
        assertEquals(400, invalidSelector.statusCode());
        assertEquals("SUBMISSION_SELECTOR_INVALID", JSON.readTree(invalidSelector.body()).get("code").asText());
        HttpResponse<String> unknownSelector = get("/submissions/status?requestId=" + requestId + "&extra=value");
        assertEquals(400, unknownSelector.statusCode());
        assertEquals("SUBMISSION_SELECTOR_INVALID", JSON.readTree(unknownSelector.body()).get("code").asText());
    }

    @Test
    void exceptionSubmissionStatusBindsArtifactAndResourceVersion() throws Exception {
        ObjectNode payload = JSON.createObjectNode();
        payload.put("apiVersion", "rulefactory.io/v1");
        payload.put("kind", "ExceptionRequest");
        payload.put("schemaVersion", "exception-request-v1");
        payload.put("status", "REQUESTED");
        payload.put("findingDigest", "sha256:" + "4".repeat(64));
        payload.put("findingFingerprint", "exception-status-fp");
        payload.put("ruleId", "corp.java.status");
        payload.put("scope", "src/**");
        payload.put("reason", "exception status recovery");
        payload.put("actor", "status-operator");
        String requestId = withEnvelopeIdentity(payload);
        String artifactDigest = payload.get("digest").asText();
        assertEquals(201, postGovernance("/exceptions", requestId, "exception-request-v1", payload.toString(),
                requestId, "status-operator", "exception status recovery").statusCode());

        JsonNode status = JSON.readTree(get("/submissions/status?artifactDigest=" + artifactDigest).body());
        assertEquals("RECEIVED_ONLY", status.get("status").asText());
        assertEquals("exceptions", status.get("resource").get("type").asText());
        assertEquals("exception-request-v1", status.get("resource").get("version").asText());
        assertEquals("REQUESTED", status.get("resource").get("state").asText());
        assertEquals(requestId, status.get("auditContext").get("resourceId").asText());
        assertTrue(!status.get("resource").has("payload"));
    }

    @Test
    void inFlightSubmissionIsPendingAndConflictingSelectorsFailClosed() throws Exception {
        BlockingJournal journal = new BlockingJournal();
        RuleOpsHttpServer pendingServer = RuleOpsHttpServer.loopback(0, new RuleOpsStore(journal));
        pendingServer.start();
        ObjectNode payload = JSON.createObjectNode();
        payload.put("apiVersion", "rulefactory.io/v1");
        payload.put("kind", "FeedbackDraft");
        payload.put("schemaVersion", "feedback-draft-v1");
        payload.put("status", "PROPOSED");
        payload.put("findingDigest", "sha256:" + "8".repeat(64));
        payload.put("findingFingerprint", "pending-status-fp");
        payload.put("ruleId", "corp.java.pending");
        payload.put("classification", "true-fp");
        payload.put("reason", "pending status test");
        payload.put("actor", "pending-operator");
        String requestId = withEnvelopeIdentity(payload);
        String feedback = payload.toString();
        CompletableFuture<HttpResponse<String>> submission = CompletableFuture.supplyAsync(() -> {
            try {
                HttpRequest request = HttpRequest.newBuilder(
                                URI.create(pendingServer.uri() + "/feedback"))
                        .header("Content-Type", "application/json")
                        .header("X-Request-Id", requestId)
                        .header("Idempotency-Key", requestId)
                        .header("X-Actor", "pending-operator")
                        .header("X-Reason", "pending status test")
                        .POST(HttpRequest.BodyPublishers.ofString(
                                "{\"id\":\"" + requestId + "\",\"version\":\"feedback-draft-v1\",\"payload\":"
                                        + feedback + "}"))
                        .build();
                return client.send(request, HttpResponse.BodyHandlers.ofString());
            } catch (Exception error) {
                throw new RuntimeException(error);
            }
        });
        try {
            assertTrue(journal.appendStarted.await(5, TimeUnit.SECONDS));
            HttpResponse<String> pending = client.send(HttpRequest.newBuilder(
                            URI.create(pendingServer.uri() + "/submissions/status?requestId=" + requestId))
                    .GET().build(), HttpResponse.BodyHandlers.ofString());
            assertEquals(200, pending.statusCode());
            JsonNode pendingStatus = JSON.readTree(pending.body());
            assertEquals("PENDING", pendingStatus.get("status").asText());
            assertEquals("UNKNOWN", pendingStatus.get("authority").asText());
            assertTrue(pendingStatus.get("resource").isNull());
            assertTrue(pendingStatus.get("auditContext").isNull());
            assertEquals(requestId, pendingStatus.get("requestId").asText());

            String otherRequestId = "sha256:" + "9".repeat(64);
            HttpResponse<String> conflict = client.send(HttpRequest.newBuilder(URI.create(
                            pendingServer.uri() + "/submissions/status?requestId=" + otherRequestId
                                    + "&artifactDigest=" + JSON.readTree(feedback).get("digest").asText()))
                    .GET().build(), HttpResponse.BodyHandlers.ofString());
            assertEquals(409, conflict.statusCode());
            assertEquals("SUBMISSION_IDENTITY_CONFLICT", JSON.readTree(conflict.body()).get("code").asText());
        } finally {
            journal.release.countDown();
            try {
                assertEquals(201, submission.join().statusCode());
            } finally {
                pendingServer.close();
            }
        }
    }

    @Test
    void ruleBundleDeliveryIsVersionedImmutableAndIndependentFromGovernanceStore() throws Exception {
        byte[] bundleBytes = "signed-rule-bundle-v1".getBytes(StandardCharsets.UTF_8);
        String bundleDigest = RuleBundleDeliveryCatalog.sha256(bundleBytes);
        RuleBundleDeliveryCatalog.DeliveryDescriptor descriptor = deliveryDescriptor(bundleDigest, "linux/amd64");
        RuleBundleDeliveryCatalog.VerificationFacts verification = verifiedDelivery();
        RuleBundleDeliveryCatalog catalog = new RuleBundleDeliveryCatalog();
        catalog.publish(descriptor, bundleBytes, verification);
        assertThrows(RuleBundleDeliveryCatalog.DeliveryException.class,
                () -> catalog.publish(descriptor, "tampered".getBytes(StandardCharsets.UTF_8), verification));
        assertThrows(RuleBundleDeliveryCatalog.DeliveryException.class,
                () -> catalog.publish(deliveryDescriptor(bundleDigest, "linux/arm64"), bundleBytes, verification));

        RuleOpsHttpServer deliveryServer = RuleOpsHttpServer.loopback(0, new RuleOpsStore(), catalog);
        deliveryServer.start();
        try {
            HttpResponse<String> response = client.send(HttpRequest.newBuilder(
                            URI.create(deliveryServer.uri() + "/rule-bundles/" + bundleDigest + "/delivery"))
                    .GET().build(), HttpResponse.BodyHandlers.ofString());
            assertEquals(200, response.statusCode());
            JsonNode json = JSON.readTree(response.body());
            assertEquals("RuleBundleDelivery", json.get("kind").asText());
            assertEquals("rule-bundle-delivery-v1", json.get("schemaVersion").asText());
            assertEquals(bundleDigest, json.get("descriptor").get("bundleDigest").asText());
            assertEquals("linux/amd64", json.get("descriptor").get("platform").asText());
            assertEquals("c2lnbmVkLXJ1bGUtYnVuZGxlLXYx", json.get("bundleBytesBase64").asText());
            assertTrue(json.get("verification").get("signatureValid").asBoolean());
            assertTrue(json.get("requestId").asText().startsWith("generated-"));

            HttpResponse<String> missing = client.send(HttpRequest.newBuilder(
                            URI.create(deliveryServer.uri() + "/rule-bundles/" + "sha256:" + "0".repeat(64) + "/delivery"))
                    .GET().build(), HttpResponse.BodyHandlers.ofString());
            assertEquals(404, missing.statusCode());
            assertEquals("BUNDLE_NOT_FOUND", JSON.readTree(missing.body()).get("code").asText());

            HttpResponse<String> invalid = client.send(HttpRequest.newBuilder(
                            URI.create(deliveryServer.uri() + "/rule-bundles/not-a-digest/delivery"))
                    .GET().build(), HttpResponse.BodyHandlers.ofString());
            assertEquals(400, invalid.statusCode());
            assertEquals("BUNDLE_INVALID", JSON.readTree(invalid.body()).get("code").asText());
        } finally {
            deliveryServer.close();
        }
    }

    @Test
    void deliveryDescriptorSnapshotsMutablePackLists() throws Exception {
        byte[] bundleBytes = "descriptor-snapshot".getBytes(StandardCharsets.UTF_8);
        String bundleDigest = RuleBundleDeliveryCatalog.sha256(bundleBytes);
        List<String> packs = new ArrayList<>(List.of("sha256:" + "5".repeat(64)));
        RuleBundleDeliveryCatalog.DeliveryDescriptor descriptor = new RuleBundleDeliveryCatalog.DeliveryDescriptor(
                "rulefactory.io/v1", "rule-bundle-delivery-v1", bundleDigest, "rule-bundle-v0",
                "java21-pmd7", "linux/amd64", "sha256:" + "1".repeat(64),
                "sha256:" + "2".repeat(64), "sha256:" + "3".repeat(64), "sha256:" + "4".repeat(64),
                packs, "sha256:" + "6".repeat(64), "sha256:" + "7".repeat(64), 10, 100);
        RuleBundleDeliveryCatalog catalog = new RuleBundleDeliveryCatalog();
        catalog.publish(descriptor, bundleBytes, verifiedDelivery());
        packs.clear();
        assertEquals(1, catalog.get(bundleDigest).orElseThrow().descriptor().runtimePackDigests().size());
    }

    @Test
    void deliveryCatalogRejectsEntriesBeyondItsMemoryBudget() throws Exception {
        RuleBundleDeliveryCatalog catalog = new RuleBundleDeliveryCatalog();
        for (int index = 0; index < RuleBundleDeliveryCatalog.MAX_CATALOG_ENTRIES; index++) {
            byte[] bytes = ("bounded-catalog-" + index).getBytes(StandardCharsets.UTF_8);
            String digest = RuleBundleDeliveryCatalog.sha256(bytes);
            catalog.publish(deliveryDescriptor(digest, "linux/amd64"), bytes, verifiedDelivery());
        }
        byte[] overflow = "bounded-catalog-overflow".getBytes(StandardCharsets.UTF_8);
        RuleBundleDeliveryCatalog.DeliveryException error = assertThrows(
                RuleBundleDeliveryCatalog.DeliveryException.class,
                () -> catalog.publish(deliveryDescriptor(RuleBundleDeliveryCatalog.sha256(overflow), "linux/amd64"),
                        overflow, verifiedDelivery()));
        assertEquals("BUNDLE_CAPACITY_EXCEEDED", error.code());
    }

    @Test
    void concurrentDeliveryReadsShareImmutableBytesWithoutTakingGovernanceJournalPath() throws Exception {
        byte[] bundleBytes = "signed-rule-bundle-concurrent".getBytes(StandardCharsets.UTF_8);
        String bundleDigest = RuleBundleDeliveryCatalog.sha256(bundleBytes);
        RuleBundleDeliveryCatalog catalog = new RuleBundleDeliveryCatalog();
        catalog.publish(deliveryDescriptor(bundleDigest, "linux/amd64"), bundleBytes, verifiedDelivery());
        RuleOpsHttpServer deliveryServer = RuleOpsHttpServer.loopback(0, new RuleOpsStore(), catalog);
        deliveryServer.start();
        try {
            List<CompletableFuture<HttpResponse<String>>> requests = new ArrayList<>();
            for (int index = 0; index < 16; index++) {
                requests.add(CompletableFuture.supplyAsync(() -> {
                    try {
                        return client.send(HttpRequest.newBuilder(
                                        URI.create(deliveryServer.uri() + "/rule-bundles/" + bundleDigest + "/delivery"))
                                .GET().build(), HttpResponse.BodyHandlers.ofString());
                    } catch (Exception error) {
                        throw new RuntimeException(error);
                    }
                }));
            }
            for (CompletableFuture<HttpResponse<String>> request : requests) {
                HttpResponse<String> response = request.join();
                assertEquals(200, response.statusCode());
                assertEquals(bundleDigest, JSON.readTree(response.body()).get("descriptor").get("bundleDigest").asText());
            }
            RuleBundleDeliveryCatalog.DeliveryMetrics metrics = catalog.metrics();
            assertEquals(16, metrics.readRequests());
            assertEquals(16, metrics.cacheHits());
            assertEquals(16, metrics.successfulResponses());
            assertTrue(metrics.maxQueueDepth() >= 0);
        } finally {
            deliveryServer.close();
        }
    }

    @Test
    void deliveryCapacityProfileCapturesLatencyPercentilesWithBoundedReadPool() throws Exception {
        byte[] bundleBytes = "capacity-profile-bundle".getBytes(StandardCharsets.UTF_8);
        String bundleDigest = RuleBundleDeliveryCatalog.sha256(bundleBytes);
        RuleBundleDeliveryCatalog catalog = new RuleBundleDeliveryCatalog();
        catalog.publish(deliveryDescriptor(bundleDigest, "linux/amd64"), bundleBytes, verifiedDelivery());
        RuleOpsHttpServer deliveryServer = RuleOpsHttpServer.loopback(0, new RuleOpsStore(), catalog);
        deliveryServer.start();
        long[] latencies = new long[128];
        try {
            for (int index = 0; index < latencies.length; index++) {
                long started = System.nanoTime();
                HttpResponse<String> response = client.send(HttpRequest.newBuilder(
                                URI.create(deliveryServer.uri() + "/rule-bundles/" + bundleDigest + "/delivery"))
                        .GET().build(), HttpResponse.BodyHandlers.ofString());
                latencies[index] = System.nanoTime() - started;
                assertEquals(200, response.statusCode());
            }
        } finally {
            deliveryServer.close();
        }
        Arrays.sort(latencies);
        long p50 = latencies[63];
        long p95 = latencies[121];
        long p99 = latencies[126];
        RuleBundleDeliveryCatalog.DeliveryMetrics metrics = catalog.metrics();
        System.out.printf("DELIVERY_CAPACITY_PROFILE clients=1 tenants=1 bundleBytes=%d requests=%d p50Ms=%.3f p95Ms=%.3f p99Ms=%.3f%n",
                bundleBytes.length, latencies.length, millis(p50), millis(p95), millis(p99));
        assertEquals(latencies.length, metrics.readRequests());
        assertEquals(latencies.length, metrics.successfulResponses());
        assertTrue(p99 < TimeUnit.SECONDS.toNanos(5), "local delivery p99 exceeded the frozen smoke bound");
    }

    private static double millis(long nanos) {
        return nanos / 1_000_000.0;
    }

    private static String withDigest(ObjectNode payload) {
        ObjectNode content = payload.deepCopy();
        content.remove("digest");
        payload.put("digest", sha256(canonical(content).toString()));
        return payload.toString();
    }

    // requestId 冻结算法（#121 / ADR-0165）：requestId = sha256(canonical(envelope 去除 requestId 与 digest))，
    // 随后按同一 canonical 约定补 digest 自字段；返回复算出的 requestId 供 envelope id / Idempotency-Key 复用。
    private static String withEnvelopeIdentity(ObjectNode payload) {
        ObjectNode envelope = payload.deepCopy();
        envelope.remove("requestId");
        envelope.remove("digest");
        String requestId = sha256(canonical(envelope).toString());
        payload.put("requestId", requestId);
        withDigest(payload);
        return requestId;
    }

    private static JsonNode canonical(JsonNode node) {
        if (node.isValueNode()) {
            return node.deepCopy();
        }
        if (node.isArray()) {
            ArrayNode result = JSON.createArrayNode();
            node.forEach(child -> result.add(canonical(child)));
            return result;
        }
        ObjectNode result = JSON.createObjectNode();
        List<String> names = new ArrayList<>();
        node.fieldNames().forEachRemaining(names::add);
        names.sort(String::compareTo);
        names.forEach(name -> result.set(name, canonical(node.get(name))));
        return result;
    }

    private static String sha256(String value) {
        try {
            return "sha256:" + java.util.HexFormat.of().formatHex(
                    MessageDigest.getInstance("SHA-256").digest(value.getBytes(StandardCharsets.UTF_8)));
        } catch (NoSuchAlgorithmException error) {
            throw new AssertionError(error);
        }
    }

    private static RuleBundleDeliveryCatalog.VerificationFacts verifiedDelivery() {
        return new RuleBundleDeliveryCatalog.VerificationFacts(true, true, true, true, true);
    }

    private static RuleBundleDeliveryCatalog.DeliveryDescriptor deliveryDescriptor(
            String bundleDigest, String platform) {
        return new RuleBundleDeliveryCatalog.DeliveryDescriptor(
                "rulefactory.io/v1",
                "rule-bundle-delivery-v1",
                bundleDigest,
                "rule-bundle-v0",
                "java21-pmd7",
                platform,
                "sha256:" + "1".repeat(64),
                "sha256:" + "2".repeat(64),
                "sha256:" + "3".repeat(64),
                "sha256:" + "4".repeat(64),
                List.of("sha256:" + "5".repeat(64)),
                "sha256:" + "6".repeat(64),
                "sha256:" + "7".repeat(64),
                10,
                100);
    }

    private static final class BlockingJournal implements RuleOpsJournal {
        private final CountDownLatch appendStarted = new CountDownLatch(1);
        private final CountDownLatch release = new CountDownLatch(1);

        @Override
        public List<Entry> read() {
            return List.of();
        }

        @Override
        public void append(Entry entry) {
            appendStarted.countDown();
            try {
                if (!release.await(5, TimeUnit.SECONDS)) {
                    throw new IllegalStateException("test journal release timed out");
                }
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                throw new IllegalStateException("test journal append interrupted", error);
            }
        }
    }

    @Test
    void plannedJobAndCoverageRoutesKeepParentIdentityInThePayload() throws Exception {
        HttpResponse<String> coverage = post("/coverage/analyze", "coverage-1", "1.0.0",
                "{\"standardId\":\"SEC-JAVA-017\"}", "coverage-1");
        assertEquals(201, coverage.statusCode());
        assertEquals("coverage", JSON.readTree(coverage.body()).get("resource").get("type").asText());

        HttpResponse<String> rule = post("/rules", "rule-1", "1.0.0", "{\"kind\":\"RuleIntent\"}", "rule-create");
        assertEquals(201, rule.statusCode());
        HttpResponse<String> job = post("/rules/rule-1/generate", "job-1", "1.0.0",
                "{\"ruleId\":\"rule-1\",\"template\":\"SymbolRule\"}", "job-1");
        assertEquals(201, job.statusCode());
        assertEquals("factory-jobs", JSON.readTree(job.body()).get("resource").get("type").asText());
    }

    @Test
    void forgedRequestIdIsRejectedFailClosed() throws Exception {
        // requestId 冻结算法（#121 / ADR-0165）：Server 独立复算 canonical envelope digest。
        ObjectNode payload = JSON.createObjectNode();
        payload.put("apiVersion", "rulefactory.io/v1");
        payload.put("kind", "FeedbackDraft");
        payload.put("schemaVersion", "feedback-draft-v1");
        payload.put("status", "PROPOSED");
        payload.put("findingDigest", "sha256:" + "9".repeat(64));
        payload.put("findingFingerprint", "forge-fp");
        payload.put("ruleId", "corp.java.forge");
        payload.put("classification", "true-fp");
        payload.put("reason", "forged identity test");
        payload.put("actor", "test-operator");
        withEnvelopeIdentity(payload);
        // 伪造一个格式正确但与 envelope 不匹配的 requestId，并把 digest 与 envelope id / Idempotency-Key
        // 全部对齐到伪造值，使唯一失败点是 requestId 内容寻址复算。
        String forgedId = "sha256:" + "c".repeat(64);
        payload.put("requestId", forgedId);
        withDigest(payload);
        HttpResponse<String> rejected = post("/feedback", forgedId, "feedback-draft-v1", payload.toString(), forgedId);
        assertEquals(400, rejected.statusCode());
        assertEquals("REQUEST_ID_MISMATCH", JSON.readTree(rejected.body()).get("code").asText());
    }

    @Test
    void registryReadsAreClosedDigestOnlyVersionedAndReadOnly() throws Exception {
        for (String type : List.of("rules", "standards")) {
            assertEquals(201, post("/" + type, "asset", "1.0.0",
                    "{\"privateText\":\"must-not-leak\"}", type + "-v1").statusCode());
            assertEquals(201, post("/" + type, "asset", "2.0.0",
                    "{\"privateText\":\"must-not-leak-either\"}", type + "-v2").statusCode());
            HttpResponse<String> firstResponse = get("/registry/" + type + "?limit=1");
            assertEquals(200, firstResponse.statusCode());
            JsonNode first = JSON.readTree(firstResponse.body());
            assertEquals(8, first.size());
            assertEquals("rulefactory.io/v1", first.get("apiVersion").asText());
            assertEquals("asset-registry-v1", first.get("schemaVersion").asText());
            assertEquals("UNKNOWN", first.get("authority").asText());
            assertEquals(1, first.get("items").size());
            assertTrue(first.get("hasMore").asBoolean());
            JsonNode item = first.get("items").get(0);
            assertEquals(5, item.size());
            assertEquals(type, item.get("type").asText());
            assertEquals("asset", item.get("id").asText());
            assertEquals("1.0.0", item.get("version").asText());
            assertTrue(item.has("state"));
            JsonNode original = JSON.readTree(get("/" + type + "/asset?version=1.0.0").body());
            assertEquals(original.get("resource").get("digest"), item.get("digest"));
            assertTrue(!firstResponse.body().contains("must-not-leak"));
            String cursor = java.net.URLEncoder.encode(first.get("nextCursor").asText(), StandardCharsets.UTF_8);
            JsonNode second = JSON.readTree(get("/registry/" + type + "?limit=1&cursor=" + cursor).body());
            assertEquals("2.0.0", second.get("items").get(0).get("version").asText());
            assertTrue(!second.get("hasMore").asBoolean());
            HttpResponse<String> detailResponse = get("/registry/" + type + "/asset?version=1.0.0");
            assertEquals(200, detailResponse.statusCode());
            JsonNode detail = JSON.readTree(detailResponse.body());
            assertEquals(5, detail.size());
            assertEquals("UNKNOWN", detail.get("authority").asText());
            assertEquals(item, detail.get("resource"));
            HttpResponse<String> ambiguous = get("/registry/" + type + "/asset");
            assertEquals(409, ambiguous.statusCode());
            assertEquals("RESOURCE_VERSION_REQUIRED", JSON.readTree(ambiguous.body()).get("code").asText());
        }
        assertEquals(4, JSON.readTree(get("/audit").body()).get("events").size());
    }

    @Test
    void registryRejectsWritesUnknownResourcesAndInvalidSelectors() throws Exception {
        assertEquals(201, post("/rules", "asset", "1.0.0", "{}", "registry-seed").statusCode());
        assertEquals(200, get("/registry/rules/asset").statusCode());
        for (String path : List.of("/registry", "/registry/policies", "/registry/rules/absent",
                "/registry/rules/asset?version=2.0.0", "/registry/rules/asset/generate")) {
            HttpResponse<String> response = get(path);
            assertEquals(404, response.statusCode(), path);
            assertEquals("NOT_FOUND", JSON.readTree(response.body()).get("code").asText());
        }
        for (String query : List.of("?version=", "?version=%20", "?version=%00",
                "?version=1&version=2", "?version=1&extra=2", "?extra=1", "?version")) {
            HttpResponse<String> response = get("/registry/rules/asset" + query);
            assertEquals(400, response.statusCode(), query);
            JsonNode error = JSON.readTree(response.body());
            assertEquals(4, error.size());
            assertTrue(error.has("requestId"));
            assertTrue(List.of("IDENTITY_INVALID", "RESOURCE_QUERY_INVALID").contains(error.get("code").asText()));
        }
        for (String query : List.of("?limit=0", "?limit=257", "?limit=1&limit=2", "?extra=1")) {
            assertEquals(400, get("/registry/rules" + query).statusCode());
        }
        for (String method : List.of("POST", "PUT", "PATCH", "DELETE")) {
            HttpResponse<String> response = client.send(HttpRequest.newBuilder(uri("/registry/rules"))
                    .method(method, HttpRequest.BodyPublishers.noBody()).build(), HttpResponse.BodyHandlers.ofString());
            assertEquals(405, response.statusCode());
            assertEquals("METHOD_NOT_ALLOWED", JSON.readTree(response.body()).get("code").asText());
        }
        JsonNode empty = JSON.readTree(get("/registry/standards").body());
        assertEquals(0, empty.get("items").size());
        assertEquals("UNKNOWN", empty.get("authority").asText());
        assertEquals(1, JSON.readTree(get("/audit").body()).get("events").size());
    }

    @Test
    void registryReadsUseTheRecoveredJournalWithoutAppending(@org.junit.jupiter.api.io.TempDir java.nio.file.Path dir)
            throws Exception {
        java.nio.file.Path path = dir.resolve("registry.jsonl");
        try (RuleOpsJournal journal = new FileRuleOpsJournal(path)) {
            RuleOpsStore store = new RuleOpsStore(journal);
            store.create(RuleOpsStore.ResourceType.RULE, "recovered", "1.0.0",
                    JSON.readTree("{\"privateText\":\"never-return\"}"), "recovered-rule", "operator", "contract test");
        }
        byte[] before = java.nio.file.Files.readAllBytes(path);
        server.close();
        try (RuleOpsJournal journal = new FileRuleOpsJournal(path)) {
            server = RuleOpsHttpServer.loopback(0, new RuleOpsStore(journal));
            server.start();
            HttpResponse<String> response = get("/registry/rules/recovered?version=1.0.0");
            assertEquals(200, response.statusCode());
            JsonNode resource = JSON.readTree(response.body()).get("resource");
            assertEquals("recovered", resource.get("id").asText());
            assertEquals(5, resource.size());
            assertTrue(!response.body().contains("never-return"));
            assertTrue(Arrays.equals(before, java.nio.file.Files.readAllBytes(path)));
        }
    }

    private HttpResponse<String> get(String path) throws Exception {
        return client.send(HttpRequest.newBuilder(uri(path)).GET().build(), HttpResponse.BodyHandlers.ofString());
    }
    private HttpResponse<String> post(String path, String id, String version, String payload, String operationId)
            throws Exception {
        HttpRequest request = HttpRequest.newBuilder(uri(path))
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "request-" + operationId)
                .header("Idempotency-Key", operationId)
                .header("X-Actor", "test-operator")
                .header("X-Reason", "contract test")
                .POST(HttpRequest.BodyPublishers.ofString(
                        "{\"id\":\"" + id + "\",\"version\":\"" + version + "\",\"payload\":" + payload + "}"))
                .build();
        return client.send(request, HttpResponse.BodyHandlers.ofString());
    }

    private HttpResponse<String> postGovernance(String path, String id, String version, String payload,
            String operationId, String actor, String reason) throws Exception {
        HttpRequest request = HttpRequest.newBuilder(uri(path))
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "request-" + operationId)
                .header("Idempotency-Key", operationId)
                .header("X-Actor", actor)
                .header("X-Reason", reason)
                .POST(HttpRequest.BodyPublishers.ofString(
                        "{\"id\":\"" + id + "\",\"version\":\"" + version + "\",\"payload\":" + payload + "}"))
                .build();
        return client.send(request, HttpResponse.BodyHandlers.ofString());
    }

    private HttpResponse<String> postWithoutHeaders(String path, String id, String version, String payload)
            throws Exception {
        HttpRequest request = HttpRequest.newBuilder(uri(path))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(
                        "{\"id\":\"" + id + "\",\"version\":\"" + version + "\",\"payload\":" + payload + "}"))
                .build();
        return client.send(request, HttpResponse.BodyHandlers.ofString());
    }

    private HttpResponse<String> transition(String path, String version, String operationId) throws Exception {
        return transitionBody(path, version == null ? "{}" : "{\"version\":\"" + version + "\"}", operationId);
    }

    private HttpResponse<String> transitionBody(String path, String body, String operationId) throws Exception {
        HttpRequest request = HttpRequest.newBuilder(uri(path))
                .header("Content-Type", "application/json")
                .header("Idempotency-Key", operationId)
                .header("X-Actor", "governance")
                .header("X-Reason", "approved lifecycle test")
                .POST(HttpRequest.BodyPublishers.ofString(body))
                .build();
        return client.send(request, HttpResponse.BodyHandlers.ofString());
    }

    private URI uri(String path) {
        return URI.create(server.uri().toString() + path);
    }
}
