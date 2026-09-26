package io.ruleops.server;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.Base64;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.LongAdder;

/**
 * Immutable, digest-addressed Server-side RuleBundle delivery catalog.
 *
 * <p>The catalog is intentionally independent from {@link RuleOpsStore}: high
 * frequency delivery reads do not take the governance journal/store lock. It
 * stores only bounded bytes and verification facts supplied by the publishing
 * boundary; it does not grant Gate or SCM authority.</p>
 */
public final class RuleBundleDeliveryCatalog {
    public static final int MAX_BUNDLE_BYTES = 128 * 1024 * 1024;
    public static final int MAX_RESPONSE_BYTES = 180005548;
    public static final int MAX_CATALOG_ENTRIES = 256;
    public static final long MAX_CATALOG_BUNDLE_BYTES = 512L * 1024 * 1024;
    private static final int MAX_RUNTIME_PACKS = 32;
    private static final int MAX_IDENTITY_TEXT_CHARS = 256;
    private static final String DIGEST_PATTERN = "sha256:[0-9a-f]{64}";
    private final Map<String, DeliveryRecord> records = new ConcurrentHashMap<>();
    private long catalogBundleBytes;
    private final LongAdder readRequests = new LongAdder();
    private final LongAdder cacheHits = new LongAdder();
    private final LongAdder cacheMisses = new LongAdder();
    private final LongAdder invalidRequests = new LongAdder();
    private final LongAdder servedBytes = new LongAdder();
    private final LongAdder successfulResponses = new LongAdder();
    private final LongAdder failedResponses = new LongAdder();
    private final LongAdder overloads = new LongAdder();
    private final AtomicInteger maxQueueDepth = new AtomicInteger();

    public synchronized void publish(DeliveryDescriptor descriptor, byte[] bundleBytes, VerificationFacts verification)
            throws DeliveryException {
        if (descriptor == null || verification == null) {
            throw new DeliveryException("BUNDLE_INVALID", "descriptor and verification facts are required");
        }
        descriptor.validate();
        if (bundleBytes == null || bundleBytes.length == 0 || bundleBytes.length > MAX_BUNDLE_BYTES) {
            throw new DeliveryException("BUNDLE_INVALID", "bundle bytes are empty or exceed the delivery limit");
        }
        String actualDigest = sha256(bundleBytes);
        if (!descriptor.bundleDigest().equals(actualDigest)) {
            throw new DeliveryException("BUNDLE_IDENTITY_CONFLICT",
                    "bundle bytes do not match descriptor bundleDigest");
        }
        DeliveryRecord existing = records.get(descriptor.bundleDigest());
        if (existing != null) {
            DeliveryRecord candidate = new DeliveryRecord(descriptor, bundleBytes, verification);
            if (!existing.sameBytes(candidate)) {
                throw new DeliveryException("BUNDLE_IDENTITY_CONFLICT",
                        "bundle digest is already published with different descriptor or bytes");
            }
            return;
        }
        if (records.size() >= MAX_CATALOG_ENTRIES
                || catalogBundleBytes > MAX_CATALOG_BUNDLE_BYTES - bundleBytes.length) {
            throw new DeliveryException("BUNDLE_CAPACITY_EXCEEDED",
                    "the delivery catalog has reached its bounded capacity");
        }
        DeliveryRecord candidate = new DeliveryRecord(descriptor, bundleBytes, verification);
        DeliveryRecord published = records.putIfAbsent(descriptor.bundleDigest(), candidate);
        if (published != null && !published.sameBytes(candidate)) {
            throw new DeliveryException("BUNDLE_IDENTITY_CONFLICT",
                    "bundle digest is already published with different descriptor or bytes");
        }
        if (published != null) {
            return;
        }
        catalogBundleBytes += bundleBytes.length;
    }

    public Optional<DeliveryRecord> get(String bundleDigest) throws DeliveryException {
        readRequests.increment();
        if (!isDigest(bundleDigest)) {
            invalidRequests.increment();
            throw new DeliveryException("BUNDLE_INVALID", "bundle digest must be a lowercase SHA-256 digest");
        }
        DeliveryRecord record = records.get(bundleDigest);
        if (record == null) {
            cacheMisses.increment();
            return Optional.empty();
        }
        cacheHits.increment();
        servedBytes.add(record.bundleBytes.length);
        return Optional.of(record.copy());
    }

    public DeliveryMetrics metrics() {
        return new DeliveryMetrics(readRequests.sum(), cacheHits.sum(), cacheMisses.sum(),
                invalidRequests.sum(), servedBytes.sum(), successfulResponses.sum(),
                failedResponses.sum(), overloads.sum(), maxQueueDepth.get());
    }

    void recordResponse(boolean success) {
        if (success) {
            successfulResponses.increment();
        } else {
            failedResponses.increment();
        }
    }

    void recordOverload() {
        overloads.increment();
    }

    void recordQueueDepth(int depth) {
        maxQueueDepth.accumulateAndGet(depth, Math::max);
    }

    public static boolean isDigest(String value) {
        return value != null && value.matches(DIGEST_PATTERN);
    }

    public static String sha256(byte[] bytes) {
        try {
            return "sha256:" + HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
        } catch (NoSuchAlgorithmException error) {
            throw new AssertionError("JDK must provide SHA-256", error);
        }
    }

    public record DeliveryMetrics(
            long readRequests,
            long cacheHits,
            long cacheMisses,
            long invalidRequests,
            long servedBytes,
            long successfulResponses,
            long failedResponses,
            long overloads,
            int maxQueueDepth) {}

    public record VerificationFacts(
            boolean signatureValid,
            boolean trustMetadataFresh,
            boolean revocationSnapshotFresh,
            boolean referencedPacksAvailable,
            boolean trustedContextValid) {

        ObjectNode asJson(ObjectMapper mapper) {
            ObjectNode result = mapper.createObjectNode();
            result.put("signatureValid", signatureValid);
            result.put("trustMetadataFresh", trustMetadataFresh);
            result.put("revocationSnapshotFresh", revocationSnapshotFresh);
            result.put("referencedPacksAvailable", referencedPacksAvailable);
            result.put("trustedContextValid", trustedContextValid);
            return result;
        }
    }

    public record DeliveryDescriptor(
            String protocolVersion,
            String descriptorVersion,
            String bundleDigest,
            String bundleSchemaVersion,
            String compatibilityIdentity,
            String platform,
            String ruleReleaseDigest,
            String policyProfileDigest,
            String executionLockDigest,
            String ontologyContractPackDigest,
            List<String> runtimePackDigests,
            String artifactStatementDigest,
            String revocationSnapshotDigest,
            long issuedAtEpochSecs,
            long expiresAtEpochSecs) {

        public DeliveryDescriptor {
            runtimePackDigests = runtimePackDigests == null ? null
                    : java.util.Collections.unmodifiableList(new java.util.ArrayList<>(runtimePackDigests));
        }

        public void validate() throws DeliveryException {
            if (!"rulefactory.io/v1".equals(protocolVersion)
                    || !"rule-bundle-delivery-v1".equals(descriptorVersion)) {
                throw new DeliveryException("BUNDLE_INVALID", "delivery protocol or descriptor version is unsupported");
            }
            for (String digest : new String[] {bundleDigest, ruleReleaseDigest, policyProfileDigest,
                    executionLockDigest, ontologyContractPackDigest, artifactStatementDigest,
                    revocationSnapshotDigest}) {
                if (!isDigest(digest)) {
                    throw new DeliveryException("BUNDLE_INVALID", "descriptor contains an invalid digest");
                }
            }
            if (runtimePackDigests == null || runtimePackDigests.isEmpty()
                    || runtimePackDigests.size() > MAX_RUNTIME_PACKS
                    || runtimePackDigests.stream().anyMatch(digest -> !isDigest(digest))) {
                throw new DeliveryException("BUNDLE_INVALID", "descriptor runtime pack digests are invalid");
            }
            if (!"rule-bundle-v0".equals(bundleSchemaVersion)) {
                throw new DeliveryException("BUNDLE_INVALID", "descriptor bundle schema is unsupported");
            }
            for (String value : new String[] {bundleSchemaVersion, compatibilityIdentity, platform}) {
                if (value == null || value.isBlank() || value.length() > MAX_IDENTITY_TEXT_CHARS
                        || value.chars().anyMatch(Character::isISOControl)) {
                    throw new DeliveryException("BUNDLE_INVALID", "descriptor identity contains empty/control text");
                }
            }
            if (expiresAtEpochSecs <= issuedAtEpochSecs) {
                throw new DeliveryException("BUNDLE_INVALID", "descriptor expiry window is invalid");
            }
        }

        ObjectNode asJson(ObjectMapper mapper) {
            ObjectNode result = mapper.createObjectNode();
            result.put("protocolVersion", protocolVersion);
            result.put("descriptorVersion", descriptorVersion);
            result.put("bundleDigest", bundleDigest);
            result.put("bundleSchemaVersion", bundleSchemaVersion);
            result.put("compatibilityIdentity", compatibilityIdentity);
            result.put("platform", platform);
            result.put("ruleReleaseDigest", ruleReleaseDigest);
            result.put("policyProfileDigest", policyProfileDigest);
            result.put("executionLockDigest", executionLockDigest);
            result.put("ontologyContractPackDigest", ontologyContractPackDigest);
            ArrayNode runtime = result.putArray("runtimePackDigests");
            runtimePackDigests.forEach(runtime::add);
            result.put("artifactStatementDigest", artifactStatementDigest);
            result.put("revocationSnapshotDigest", revocationSnapshotDigest);
            result.put("issuedAtEpochSecs", issuedAtEpochSecs);
            result.put("expiresAtEpochSecs", expiresAtEpochSecs);
            return result;
        }
    }

    public record DeliveryRecord(
            DeliveryDescriptor descriptor,
            byte[] bundleBytes,
            VerificationFacts verification) {

        public DeliveryRecord {
            bundleBytes = bundleBytes.clone();
        }

        DeliveryRecord copy() {
            return new DeliveryRecord(descriptor, bundleBytes, verification);
        }

        boolean sameBytes(DeliveryRecord other) {
            return descriptor.equals(other.descriptor)
                    && verification.equals(other.verification)
                    && java.util.Arrays.equals(bundleBytes, other.bundleBytes);
        }

        ObjectNode asJson(ObjectMapper mapper) {
            ObjectNode result = mapper.createObjectNode();
            result.put("kind", "RuleBundleDelivery");
            result.put("schemaVersion", "rule-bundle-delivery-v1");
            result.set("descriptor", descriptor.asJson(mapper));
            result.put("bundleBytesBase64", Base64.getEncoder().encodeToString(bundleBytes));
            result.set("verification", verification.asJson(mapper));
            return result;
        }
    }

    public static final class DeliveryException extends Exception {
        private final String code;

        public DeliveryException(String code, String message) {
            super(message);
            this.code = code;
        }

        public String code() {
            return code;
        }
    }
}
