package io.ruleops.authoring;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import io.ruleops.authoring.retrieval.AuthoringRetrieval;
import io.ruleops.authoring.retrieval.AuthoringRetrieval.Document;
import io.ruleops.authoring.retrieval.AuthoringRetrieval.Index;
import io.ruleops.authoring.retrieval.AuthoringRetrieval.IndexIdentity;
import io.ruleops.authoring.retrieval.AuthoringRetrieval.LifecycleState;
import io.ruleops.authoring.retrieval.AuthoringRetrieval.MetadataFilter;
import io.ruleops.authoring.retrieval.AuthoringRetrieval.Query;
import java.util.List;
import java.util.Set;
import org.junit.jupiter.api.Test;

/** Contract tests for exact lookup, metadata filters, BM25 baseline and ACL. */
class AuthoringRetrievalTest {

    private static String digest(char value) {
        return "sha256:" + String.valueOf(value).repeat(64);
    }

    private static IndexIdentity identity() {
        return new IndexIdentity(digest('a'), digest('b'), "generation-1");
    }

    private static Document document(
            String id, char digest, String title, String approvedText,
            String acl, LifecycleState state, String backend) {
        return new Document(
                id,
                "1.0.0",
                digest(digest),
                "RuleIntent",
                "SEC-JAVA-017",
                "java",
                backend,
                "VERIFIED",
                Set.of(acl),
                state,
                title,
                approvedText,
                id + "@1.0.0#" + digest(digest),
                "approved-metadata");
    }

    private static Query query(String text, Set<String> acl) {
        return new Query(
                null,
                text,
                new MetadataFilter(null, null, "java", null, null),
                acl,
                digest('c'),
                "approved-metadata",
                Set.of("SEC-JAVA-017"),
                5);
    }

    private static Index index() {
        return AuthoringRetrieval.build(identity(), List.of(
                document("corp.java.debug", 'd', "Avoid debug methods", "Java method naming and production diagnostics", "team-a", LifecycleState.ACTIVE, "pmd-java"),
                document("corp.java.no-system-exit", 'e', "Block System.exit calls", "Java API process termination and exit", "team-a", LifecycleState.ACTIVE, "pmd-java"),
                document("corp.java.revoked", 'f', "Revoked asset", "System exit", "team-a", LifecycleState.REVOKED, "pmd-java"),
                document("corp.java.private", '1', "Private asset", "System exit", "team-b", LifecycleState.ACTIVE, "pmd-java")));
    }

    @Test
    void exactLookupAndBm25AreDeterministicAndProduceUntrustedSuggestions() {
        Index index = index();
        Query exact = new Query(
                "corp.java.no-system-exit", "", null, Set.of("team-a"), digest('c'),
                "approved-metadata", Set.of("SEC-JAVA-017"), 5);
        var exactResult = index.search(exact);
        assertEquals(List.of("corp.java.no-system-exit"), exactResult.hits().stream()
                .map(hit -> hit.document().authorityId()).toList());
        assertTrue(exactResult.suggestion().orElseThrow().trust().usableForGate() == false);
        assertEquals(AuthoringRetrieval.CAPABILITY,
                exactResult.suggestion().orElseThrow().source().capability());

        var first = index.search(query("System exit", Set.of("team-a")));
        var second = index.search(query("System exit", Set.of("team-a")));
        assertEquals(first.hits().stream().map(hit -> hit.document().authorityId()).toList(),
                second.hits().stream().map(hit -> hit.document().authorityId()).toList());
        assertEquals(first.suggestion().orElseThrow().digest(), second.suggestion().orElseThrow().digest());
        assertEquals("corp.java.no-system-exit", first.hits().get(0).document().authorityId());
    }

    @Test
    void metadataFiltersAndAclExcludeRevokedAndUnauthorizedAssets() {
        Index index = index();
        Query metadata = new Query(
                null, "Java", new MetadataFilter(null, "SEC-JAVA-017", "java", "pmd-java", "VERIFIED"),
                Set.of("team-a"), digest('c'), "approved-metadata", Set.of("SEC-JAVA-017"), 10);
        assertEquals(Set.of("corp.java.debug", "corp.java.no-system-exit"),
                index.search(metadata).hits().stream().map(hit -> hit.document().authorityId()).collect(java.util.stream.Collectors.toSet()));

        assertTrue(index.search(query("System exit", Set.of("team-c"))).hits().isEmpty());
        assertTrue(index.search(new Query("corp.java.revoked", "", null, Set.of("team-a"), digest('c'),
                "approved-metadata", Set.of("SEC-JAVA-017"), 1)).hits().isEmpty());
        assertTrue(index.search(query("Private asset", Set.of("team-a"))).hits().isEmpty());
    }

    @Test
    void indexRejectsDuplicateIdentityAndMissingQueryIdentity() {
        Document first = document("corp.java.duplicate", '2', "Duplicate", "metadata", "team-a", LifecycleState.ACTIVE, "pmd-java");
        Document second = document("corp.java.duplicate", '3', "Duplicate", "metadata", "team-a", LifecycleState.RETIRED, "pmd-java");
        assertThrows(IllegalArgumentException.class, () -> AuthoringRetrieval.build(identity(), List.of(first, second)));
        assertThrows(IllegalArgumentException.class, () -> new Query(null, "text", null, Set.of("team-a"),
                "not-a-digest", "approved-metadata", Set.of("SEC-JAVA-017"), 1));
    }

    @Test
    void indexDigestBindsSearchableContentAndAcl() {
        Index first = AuthoringRetrieval.build(identity(), List.of(
                document("corp.java.bound", '4', "Original title", "Original approved summary", "team-a", LifecycleState.ACTIVE, "pmd-java")));
        Index changed = AuthoringRetrieval.build(identity(), List.of(
                document("corp.java.bound", '4', "Changed title", "Original approved summary", "team-a", LifecycleState.ACTIVE, "pmd-java")));
        Index aclChanged = AuthoringRetrieval.build(identity(), List.of(
                document("corp.java.bound", '4', "Original title", "Original approved summary", "team-b", LifecycleState.ACTIVE, "pmd-java")));

        assertTrue(!first.digest().equals(changed.digest()));
        assertTrue(!first.digest().equals(aclChanged.digest()));
    }
}
