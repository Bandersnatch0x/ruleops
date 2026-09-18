package io.ruleops.authoring.retrieval;

import io.ruleops.authoring.suggestion.AuthoringSuggestion;
import io.ruleops.authoring.suggestion.AuthoringSuggestion.Suggestion;
import io.ruleops.authoring.suggestion.AuthoringSuggestion.SuggestionType;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * Small in-memory Authoring Retrieval baseline. It supports exact identity
 * lookup, metadata filtering and deterministic lexical BM25 scoring. The
 * index accepts governed metadata and approved summaries only; source files,
 * credentials and full Finding details are outside this boundary.
 */
public final class AuthoringRetrieval {

    public static final String CAPABILITY = "hybrid-retrieval";
    public static final String CAPABILITY_VERSION = "0.1.0";
    private static final Pattern DIGEST = Pattern.compile("^sha256:[0-9a-f]{64}$");
    private static final Pattern TOKEN = Pattern.compile("[\\p{L}\\p{N}]+");
    private static final double BM25_K1 = 1.2d;
    private static final double BM25_B = 0.75d;

    public record IndexIdentity(String sourceSnapshot, String configDigest, String generation) {
        public IndexIdentity {
            digest("sourceSnapshot", sourceSnapshot);
            digest("configDigest", configDigest);
            requiredText("generation", generation);
        }
    }

    /**
     * One searchable governed asset. {@code approvedText} is an intentionally
     * bounded metadata/summary field, never a repository source-file field.
     */
    public record Document(
            String authorityId,
            String version,
            String digest,
            String kind,
            String standardId,
            String language,
            String backend,
            String maturity,
            Set<String> aclScopes,
            LifecycleState lifecycleState,
            String title,
            String approvedText,
            String sourceRef,
            String permittedDataClass) {

        public Document {
            requiredText("authorityId", authorityId);
            requiredText("version", version);
            AuthoringRetrieval.digest("digest", digest);
            requiredText("kind", kind);
            requiredText("standardId", standardId);
            requiredText("language", language);
            optionalText("backend", backend);
            requiredText("maturity", maturity);
            if (aclScopes == null || aclScopes.isEmpty()) {
                throw new IllegalArgumentException("aclScopes must not be empty");
            }
            aclScopes = aclScopes.stream().map(value -> {
                requiredText("aclScope", value);
                return value;
            }).collect(java.util.stream.Collectors.toUnmodifiableSet());
            lifecycleState = Objects.requireNonNull(lifecycleState, "lifecycleState");
            requiredText("title", title);
            requiredText("approvedText", approvedText);
            if (approvedText.length() > 64 * 1024) {
                throw new IllegalArgumentException("approvedText exceeds 64 KiB");
            }
            requiredText("sourceRef", sourceRef);
            requiredText("permittedDataClass", permittedDataClass);
        }
    }

    public enum LifecycleState {
        ACTIVE,
        RETIRED,
        REVOKED
    }

    public record MetadataFilter(
            String kind,
            String standardId,
            String language,
            String backend,
            String maturity) {

        public MetadataFilter {
            optionalText("kind", kind);
            optionalText("standardId", standardId);
            optionalText("language", language);
            optionalText("backend", backend);
            optionalText("maturity", maturity);
        }

        boolean matches(Document document) {
            return matches(kind, document.kind())
                    && matches(standardId, document.standardId())
                    && matches(language, document.language())
                    && matches(backend, document.backend())
                    && matches(maturity, document.maturity());
        }

        private static boolean matches(String expected, String actual) {
            return expected == null || expected.equals(actual);
        }
    }

    public record Query(
            String exactAuthorityId,
            String text,
            MetadataFilter metadata,
            Set<String> callerAclScopes,
            String repositoryIdentity,
            String permittedDataClass,
            Set<String> standardIds,
            int topK) {

        public Query {
            optionalText("exactAuthorityId", exactAuthorityId);
            text = text == null ? "" : text;
            if (text.chars().anyMatch(Character::isISOControl)) {
                throw new IllegalArgumentException("text must be control-free");
            }
            metadata = metadata == null ? new MetadataFilter(null, null, null, null, null) : metadata;
            if (callerAclScopes == null || callerAclScopes.isEmpty()) {
                throw new IllegalArgumentException("callerAclScopes must not be empty");
            }
            callerAclScopes = callerAclScopes.stream().map(value -> {
                requiredText("callerAclScope", value);
                return value;
            }).collect(java.util.stream.Collectors.toUnmodifiableSet());
            digest("repositoryIdentity", repositoryIdentity);
            requiredText("permittedDataClass", permittedDataClass);
            if (standardIds == null || standardIds.isEmpty()) {
                throw new IllegalArgumentException("standardIds must not be empty");
            }
            standardIds = standardIds.stream().map(value -> {
                requiredText("standardId", value);
                return value;
            }).collect(java.util.stream.Collectors.toUnmodifiableSet());
            if (topK < 1 || topK > 100) {
                throw new IllegalArgumentException("topK must be between 1 and 100");
            }
        }
    }

    public record SearchHit(Document document, double score) {}

    public record Response(List<SearchHit> hits, Optional<AuthoringSuggestion> suggestion) {
        public Response {
            hits = List.copyOf(hits);
            suggestion = Objects.requireNonNull(suggestion, "suggestion");
            if (hits.isEmpty() && suggestion.isPresent()) {
                throw new IllegalArgumentException("empty retrieval result cannot carry a suggestion");
            }
        }
    }

    private record IndexedDocument(Document document, List<String> tokens, Map<String, Integer> frequencies) {}

    /** Immutable index generation. Rebuild with a new identity rather than mutating it. */
    public static final class Index {
        private final IndexIdentity identity;
        private final List<IndexedDocument> documents;
        private final Map<String, IndexedDocument> activeById;
        private final Map<String, Integer> documentFrequency;
        private final double averageDocumentLength;

        private Index(IndexIdentity identity, List<IndexedDocument> documents) {
            this.identity = identity;
            this.documents = List.copyOf(documents);
            this.activeById = new LinkedHashMap<>();
            this.documentFrequency = new HashMap<>();
            int totalLength = 0;
            for (IndexedDocument document : documents) {
                if (document.document().lifecycleState() != LifecycleState.ACTIVE) continue;
                IndexedDocument old = activeById.put(document.document().authorityId(), document);
                if (old != null) {
                    throw new IllegalArgumentException(
                            "multiple ACTIVE versions for authority id: " + document.document().authorityId());
                }
                totalLength += document.tokens().size();
                for (String token : document.frequencies().keySet()) {
                    documentFrequency.merge(token, 1, Integer::sum);
                }
            }
            this.averageDocumentLength = activeById.isEmpty()
                    ? 0d : (double) totalLength / activeById.size();
        }

        public IndexIdentity identity() {
            return identity;
        }

        public String digest() {
            StringBuilder canonical = new StringBuilder(identity.sourceSnapshot())
                    .append('|').append(identity.configDigest())
                    .append('|').append(identity.generation()).append('\n');
            for (IndexedDocument indexed : documents) {
                Document document = indexed.document();
                canonical.append(document.authorityId()).append('|')
                        .append(document.version()).append('|')
                        .append(document.digest()).append('|')
                        .append(document.kind()).append('|')
                        .append(document.standardId()).append('|')
                        .append(document.language()).append('|')
                        .append(document.backend()).append('|')
                        .append(document.maturity()).append('|')
                        .append(document.lifecycleState()).append('|')
                        .append(String.join(",", document.aclScopes().stream().sorted().toList())).append('|')
                        .append(document.title()).append('|')
                        .append(document.approvedText()).append('|')
                        .append(document.sourceRef()).append('|')
                        .append(document.permittedDataClass()).append('\n');
            }
            return "sha256:" + HexFormat.of().formatHex(sha256(canonical.toString().getBytes(StandardCharsets.UTF_8)));
        }

        /** Exact lookup respects lifecycle, ACL, data class and metadata scope. */
        public Response search(Query query) {
            Objects.requireNonNull(query, "query");
            if (query.exactAuthorityId() != null) {
                IndexedDocument indexed = activeById.get(query.exactAuthorityId());
                if (indexed != null && visible(indexed.document(), query)) {
                    return response(query, List.of(new SearchHit(indexed.document(), 1d)));
                }
                return new Response(List.of(), Optional.empty());
            }

            List<String> queryTokens = tokenize(query.text());
            if (queryTokens.isEmpty()) {
                return new Response(List.of(), Optional.empty());
            }
            Set<String> uniqueQueryTokens = new LinkedHashSet<>(queryTokens);
            List<SearchHit> hits = new ArrayList<>();
            for (IndexedDocument indexed : documents) {
                Document document = indexed.document();
                if (!visible(document, query)) continue;
                double score = score(indexed, uniqueQueryTokens);
                if (score > 0d) hits.add(new SearchHit(document, score));
            }
            hits.sort(Comparator.comparingDouble(SearchHit::score).reversed()
                    .thenComparing(hit -> hit.document().authorityId())
                    .thenComparing(hit -> hit.document().version())
                    .thenComparing(hit -> hit.document().digest()));
            if (hits.size() > query.topK()) hits = new ArrayList<>(hits.subList(0, query.topK()));
            return response(query, hits);
        }

        private Response response(Query query, List<SearchHit> hits) {
            if (hits.isEmpty()) return new Response(List.of(), Optional.empty());
            List<Suggestion> suggestions = hits.stream()
                    .map(hit -> new Suggestion(
                            SuggestionType.RELATED_ASSET,
                            hit.document().title(),
                            List.of(hit.document().sourceRef()),
                            null))
                    .toList();
            AuthoringSuggestion suggestion = new AuthoringSuggestion(
                    AuthoringSuggestion.KIND,
                    AuthoringSuggestion.SCHEMA_VERSION,
                    new AuthoringSuggestion.Source(
                            CAPABILITY,
                            CAPABILITY_VERSION,
                            identity.sourceSnapshot(),
                            identity.configDigest(),
                            null,
                            identity.generation()),
                    new AuthoringSuggestion.Scope(
                            query.standardIds().stream().sorted().toList(),
                            query.repositoryIdentity(),
                            query.permittedDataClass()),
                    suggestions,
                    AuthoringSuggestion.Trust.untrusted());
            suggestion.validateSourceRefs(hits.stream()
                    .map(hit -> hit.document().sourceRef())
                    .collect(java.util.stream.Collectors.toUnmodifiableSet()));
            return new Response(hits, Optional.of(suggestion));
        }

        private double score(IndexedDocument indexed, Set<String> queryTokens) {
            if (averageDocumentLength == 0d) return 0d;
            double score = 0d;
            for (String token : queryTokens) {
                Integer frequency = indexed.frequencies().get(token);
                Integer documentCount = documentFrequency.get(token);
                if (frequency == null || documentCount == null) continue;
                double idf = Math.log(1d + (activeById.size() - documentCount + 0.5d)
                        / (documentCount + 0.5d));
                double denominator = frequency + BM25_K1
                        * (1d - BM25_B + BM25_B * indexed.tokens().size() / averageDocumentLength);
                score += idf * (frequency * (BM25_K1 + 1d)) / denominator;
            }
            return score;
        }

        private static boolean visible(Document document, Query query) {
            return document.lifecycleState() == LifecycleState.ACTIVE
                    && query.standardIds().contains(document.standardId())
                    && query.permittedDataClass().equals(document.permittedDataClass())
                    && document.aclScopes().stream().anyMatch(query.callerAclScopes()::contains)
                    && query.metadata().matches(document);
        }
    }

    public static Index build(IndexIdentity identity, Collection<Document> sourceDocuments) {
        Objects.requireNonNull(identity, "identity");
        Objects.requireNonNull(sourceDocuments, "sourceDocuments");
        List<Document> documents = new ArrayList<>(sourceDocuments);
        documents.forEach(value -> Objects.requireNonNull(value, "document"));
        documents.sort(Comparator.comparing(Document::authorityId)
                .thenComparing(Document::version)
                .thenComparing(Document::digest));
        Set<String> identities = new HashSet<>();
        List<IndexedDocument> indexed = new ArrayList<>();
        for (Document document : documents) {
            String identityKey = document.authorityId() + "\u0000" + document.version();
            if (!identities.add(identityKey)) {
                throw new IllegalArgumentException("duplicate document identity: " + identityKey);
            }
            List<String> tokens = tokenize(String.join(" ",
                    document.authorityId(), document.version(), document.kind(), document.standardId(),
                    document.language(), document.backend(), document.maturity(), document.title(),
                    document.approvedText()));
            Map<String, Integer> frequencies = new HashMap<>();
            tokens.forEach(token -> frequencies.merge(token, 1, Integer::sum));
            indexed.add(new IndexedDocument(document, List.copyOf(tokens), Map.copyOf(frequencies)));
        }
        return new Index(identity, indexed);
    }

    private static List<String> tokenize(String text) {
        List<String> tokens = new ArrayList<>();
        var matcher = TOKEN.matcher(text.toLowerCase(Locale.ROOT));
        while (matcher.find()) tokens.add(matcher.group());
        return List.copyOf(tokens);
    }

    private static byte[] sha256(byte[] bytes) {
        try {
            return MessageDigest.getInstance("SHA-256").digest(bytes);
        } catch (NoSuchAlgorithmException e) {
            throw new AssertionError("JDK must provide SHA-256", e);
        }
    }

    private static void requiredText(String name, String value) {
        if (value == null || value.isBlank() || value.chars().anyMatch(Character::isISOControl)) {
            throw new IllegalArgumentException(name + " must be non-empty and control-free");
        }
    }

    private static void optionalText(String name, String value) {
        if (value != null && value.chars().anyMatch(Character::isISOControl)) {
            throw new IllegalArgumentException(name + " must be control-free");
        }
    }

    private static void digest(String name, String value) {
        requiredText(name, value);
        if (!DIGEST.matcher(value).matches()) {
            throw new IllegalArgumentException(name + " must be sha256:<64 lowercase hex characters>");
        }
    }

    private AuthoringRetrieval() {}
}
