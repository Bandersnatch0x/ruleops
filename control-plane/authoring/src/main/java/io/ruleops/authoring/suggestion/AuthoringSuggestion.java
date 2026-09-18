package io.ruleops.authoring.suggestion;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HexFormat;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Objects;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * Non-authoritative output shared by the Authoring Orchestrator and retrieval
 * perimeter. It is deliberately a separate type from Finding, EvidenceBundle,
 * RuleRelease and GateDecision, so a suggestion cannot enter the execution
 * path by structural accident.
 *
 * <p>The object is an in-memory representation of the
 * {@code erf.authoring/v0alpha1} contract. It contains only provenance,
 * metadata and caller-permitted suggestion text; it does not contain source
 * files or engine observations.</p>
 */
public record AuthoringSuggestion(
        String kind,
        String schemaVersion,
        Source source,
        Scope scope,
        List<Suggestion> suggestions,
        Trust trust) {

    public static final String KIND = "AuthoringSuggestion";
    public static final String SCHEMA_VERSION = "erf.authoring/v0alpha1";
    private static final Pattern DIGEST = Pattern.compile("^sha256:[0-9a-f]{64}$");

    public AuthoringSuggestion {
        if (!KIND.equals(kind)) {
            throw new IllegalArgumentException("kind must be " + KIND);
        }
        if (!SCHEMA_VERSION.equals(schemaVersion)) {
            throw new IllegalArgumentException("schemaVersion must be " + SCHEMA_VERSION);
        }
        source = Objects.requireNonNull(source, "source");
        scope = Objects.requireNonNull(scope, "scope");
        if (suggestions == null || suggestions.isEmpty()) {
            throw new IllegalArgumentException("suggestions must not be empty");
        }
        suggestions = suggestions.stream()
                .map(Objects::requireNonNull)
                .sorted(Comparator.comparing((Suggestion value) -> value.type().wireName())
                        .thenComparing(Suggestion::content)
                        .thenComparing(value -> String.join("\u0000", value.sourceRefs())))
                .toList();
        trust = Objects.requireNonNull(trust, "trust");
        if (trust.authoritative() || trust.independentlyReviewed() || trust.usableForGate()) {
            throw new IllegalArgumentException("AuthoringSuggestion trust flags must all be false");
        }
    }

    /** Validate that every provenance reference is in the caller ACL set. */
    public void validateSourceRefs(Set<String> permittedSourceRefs) {
        Objects.requireNonNull(permittedSourceRefs, "permittedSourceRefs");
        for (Suggestion suggestion : suggestions) {
            if (!permittedSourceRefs.containsAll(suggestion.sourceRefs())) {
                throw new IllegalArgumentException(
                        "suggestion contains a source reference outside the caller ACL");
            }
        }
    }

    /** Stable digest for evidence and idempotent transport adapters. */
    public String digest() {
        try {
            return "sha256:" + HexFormat.of().formatHex(
                    MessageDigest.getInstance("SHA-256").digest(canonicalBytes()));
        } catch (NoSuchAlgorithmException e) {
            throw new AssertionError("JDK must provide SHA-256", e);
        }
    }

    /**
     * Deterministic, length-delimited representation of the contract. It is
     * intentionally not a Gate or Finding serialization format.
     */
    public byte[] canonicalBytes() {
        List<String> fields = new ArrayList<>();
        fields.add(kind);
        fields.add(schemaVersion);
        fields.add(source.capability());
        fields.add(source.capabilityVersion());
        fields.add(source.sourceSnapshot());
        fields.add(source.configDigest());
        fields.add(source.modelIdentity());
        fields.add(source.indexGeneration());
        fields.addAll(scope.standardIds());
        fields.add(scope.repositoryIdentity());
        fields.add(scope.permittedDataClass());
        for (Suggestion suggestion : suggestions) {
            fields.add(suggestion.type().wireName());
            fields.add(suggestion.content());
            fields.addAll(suggestion.sourceRefs());
            fields.add(suggestion.confidence() == null ? null : suggestion.confidence().toString());
        }
        fields.add(Boolean.toString(trust.authoritative()));
        fields.add(Boolean.toString(trust.independentlyReviewed()));
        fields.add(Boolean.toString(trust.usableForGate()));

        int size = 0;
        for (String field : fields) {
            size += Integer.BYTES + (field == null ? 0 : field.getBytes(StandardCharsets.UTF_8).length);
        }
        byte[] output = new byte[size];
        int offset = 0;
        for (String field : fields) {
            byte[] bytes = field == null ? new byte[0] : field.getBytes(StandardCharsets.UTF_8);
            int length = field == null ? -1 : bytes.length;
            output[offset++] = (byte) (length >>> 24);
            output[offset++] = (byte) (length >>> 16);
            output[offset++] = (byte) (length >>> 8);
            output[offset++] = (byte) length;
            System.arraycopy(bytes, 0, output, offset, bytes.length);
            offset += bytes.length;
        }
        return output;
    }

    public record Source(
            String capability,
            String capabilityVersion,
            String sourceSnapshot,
            String configDigest,
            String modelIdentity,
            String indexGeneration) {

        public Source {
            requiredText("capability", capability);
            requiredText("capabilityVersion", capabilityVersion);
            digest("sourceSnapshot", sourceSnapshot);
            digest("configDigest", configDigest);
            optionalText("modelIdentity", modelIdentity);
            optionalText("indexGeneration", indexGeneration);
        }
    }

    public record Scope(
            List<String> standardIds,
            String repositoryIdentity,
            String permittedDataClass) {

        public Scope {
            if (standardIds == null || standardIds.isEmpty()) {
                throw new IllegalArgumentException("scope.standardIds must not be empty");
            }
            LinkedHashSet<String> unique = new LinkedHashSet<>();
            for (String standardId : standardIds) {
                requiredText("standardId", standardId);
                unique.add(standardId);
            }
            standardIds = unique.stream().sorted().toList();
            digest("repositoryIdentity", repositoryIdentity);
            requiredText("permittedDataClass", permittedDataClass);
        }
    }

    public record Suggestion(
            SuggestionType type,
            String content,
            List<String> sourceRefs,
            Double confidence) {

        public Suggestion {
            type = Objects.requireNonNull(type, "type");
            requiredText("content", content);
            if (content.length() > 16_384) {
                throw new IllegalArgumentException("suggestion content exceeds 16 KiB");
            }
            if (sourceRefs == null || sourceRefs.isEmpty()) {
                throw new IllegalArgumentException("suggestion.sourceRefs must not be empty");
            }
            LinkedHashSet<String> unique = new LinkedHashSet<>();
            for (String sourceRef : sourceRefs) {
                requiredText("sourceRef", sourceRef);
                unique.add(sourceRef);
            }
            sourceRefs = unique.stream().sorted().toList();
            if (confidence != null) {
                throw new IllegalArgumentException("confidence must be null for untrusted suggestions");
            }
        }
    }

    public enum SuggestionType {
        SCENARIO("scenario"),
        POSITIVE("positive"),
        NEGATIVE("negative"),
        NEAR_MISS("near_miss"),
        EXPLANATION("explanation"),
        RELATED_ASSET("related_asset");

        private final String wireName;

        SuggestionType(String wireName) {
            this.wireName = wireName;
        }

        public String wireName() {
            return wireName;
        }
    }

    public record Trust(boolean authoritative, boolean independentlyReviewed, boolean usableForGate) {
        public static Trust untrusted() {
            return new Trust(false, false, false);
        }
    }

    private static void requiredText(String name, String value) {
        if (value == null || value.isBlank() || value.chars().anyMatch(Character::isISOControl)) {
            throw new IllegalArgumentException(name + " must be non-empty and control-free");
        }
    }

    private static void optionalText(String name, String value) {
        if (value != null) {
            requiredText(name, value);
        }
    }

    private static void digest(String name, String value) {
        requiredText(name, value);
        if (!DIGEST.matcher(value).matches()) {
            throw new IllegalArgumentException(name + " must be sha256:<64 lowercase hex characters>");
        }
    }
}
