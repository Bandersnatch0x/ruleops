package io.ruleops.verification;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/**
 * Local verification semantics: bad fixtures must be hit, good fixtures must
 * stay clean; a FAIL verdict must still produce reviewable per-fixture
 * evidence.
 */
class LocalVerificationTest {

    private static final String RULESET = """
            <?xml version="1.0" encoding="UTF-8"?>
            <ruleset name="erf-test"
                     xmlns="http://pmd.sourceforge.net/ruleset/2.0.0">
              <description>test</description>
              <!-- message is load-bearing: PMD drops violations of rules
                   without a message attribute (observed on 7.9.0) -->
              <rule name="T1" language="java" message="m" class="net.sourceforge.pmd.lang.rule.xpath.XPathRule">
                <description>d</description>
                <properties>
                  <property name="xpath">
                    <value><![CDATA[%s]]></value>
                  </property>
                </properties>
              </rule>
            </ruleset>
            """;

    private static final String BAD_SNIPPET = "package fixtures;\n"
            + "public final class Bad0 {\n"
            + "    void example() {\n"
            + "        System.exit(1);\n"
            + "    }\n"
            + "}\n";

    private static final String GOOD_SNIPPET = "package fixtures;\n"
            + "public final class Good0 {\n"
            + "    void example() {\n"
            + "        int ok = 1;\n"
            + "    }\n"
            + "}\n";

    @Test
    void badFixturesHitAndGoodFixturesStayClean(@TempDir Path tmp) throws Exception {
        Path candidate = Path.of("/tmp/tc-unit");
        writeCandidate(candidate, "//MethodCall[pmd-java:matchesSig('java.lang.System#exit(int)')]");

        EvidenceBundle bundle = new LocalVerification().verify(candidate);
        assertEquals("PASS", bundle.verdict());
        assertEquals("LOCAL_UNATTESTED", bundle.trustLevel());
        assertEquals(4, bundle.fixtures().size());
        assertTrue(bundle.fixtures().stream().allMatch(EvidenceBundle.FixtureResult::pass));
        assertEquals(List.of("bad", "good", "holdout", "near-miss"),
                bundle.fixtures().stream().map(EvidenceBundle.FixtureResult::category).sorted().toList());
    }

    @Test
    void failingExpectationProducesFailWithEvidence(@TempDir Path candidate) throws Exception {
        // A rule that never matches: bad fixture stays unhit -> FAIL.
        writeCandidate(candidate, "//NeverExistingNode[@Foo='bar']");

        EvidenceBundle bundle = new LocalVerification().verify(candidate);
        assertEquals("FAIL", bundle.verdict());
        EvidenceBundle.FixtureResult bad = bundle.fixtures().stream()
                .filter(f -> f.fixture().startsWith("bad/"))
                .findFirst()
                .orElseThrow();
        assertEquals(0, bad.actualHits());
        org.junit.jupiter.api.Assertions.assertFalse(bad.pass());
        assertTrue(bundle.notes().contains("violated"));
    }

    @Test
    void tamperedContentAddressedCandidateIsRejected(@TempDir Path candidate) throws Exception {
        writeCandidate(candidate, "//MethodCall[pmd-java:matchesSig('java.lang.System#exit(int)')]");
        Files.writeString(candidate.resolve("manifest.json"), "{\"digest\":\"deadbeef\"}\n");
        org.junit.jupiter.api.Assertions.assertThrows(java.io.IOException.class,
                () -> new LocalVerification().verify(candidate));
    }

    @Test
    void nearMissFalsePositiveIsReviewable(@TempDir Path candidate) throws Exception {
        writeCandidate(candidate, "//MethodCall[pmd-java:matchesSig('java.lang.System#getSecurityManager()')]");

        EvidenceBundle bundle = new LocalVerification().verify(candidate);
        EvidenceBundle.FixtureResult nearMiss = bundle.fixtures().stream()
                .filter(f -> f.category().equals("near-miss"))
                .findFirst().orElseThrow();
        assertEquals("must-be-clean", nearMiss.expectation());
        assertEquals(1, nearMiss.actualHits());
        org.junit.jupiter.api.Assertions.assertFalse(nearMiss.pass());
        assertEquals("FAIL", bundle.verdict());
    }

    @Test
    void missedHoldoutIsReviewable(@TempDir Path candidate) throws Exception {
        writeCandidate(candidate, "//NeverExistingNode[@Foo='bar']");

        EvidenceBundle bundle = new LocalVerification().verify(candidate);
        EvidenceBundle.FixtureResult holdout = bundle.fixtures().stream()
                .filter(f -> f.category().equals("holdout"))
                .findFirst().orElseThrow();
        assertEquals("must-hit", holdout.expectation());
        assertEquals(0, holdout.actualHits());
        org.junit.jupiter.api.Assertions.assertFalse(holdout.pass());
        assertEquals("FAIL", bundle.verdict());
    }

    @Test
    void unknownFixtureCategoryFailsClosed(@TempDir Path candidate) throws Exception {
        writeCandidate(candidate, "//MethodCall[pmd-java:matchesSig('java.lang.System#exit(int)')]");
        Files.createDirectories(candidate.resolve("fixtures/unknown"));
        Files.writeString(candidate.resolve("fixtures/unknown/Unknown0.java"), GOOD_SNIPPET);

        org.junit.jupiter.api.Assertions.assertThrows(java.io.IOException.class,
                () -> new LocalVerification().verify(candidate));
    }

    private static void writeCandidate(Path candidate, String xpath) throws Exception {
        Files.createDirectories(candidate.resolve("fixtures/bad"));
        Files.createDirectories(candidate.resolve("fixtures/good"));
        Files.createDirectories(candidate.resolve("fixtures/holdout"));
        Files.createDirectories(candidate.resolve("fixtures/near-miss"));
        Files.writeString(candidate.resolve("ruleset.xml"), RULESET.formatted(xpath));
        Files.writeString(candidate.resolve("fixtures/bad/Bad0.java"), BAD_SNIPPET);
        Files.writeString(candidate.resolve("fixtures/good/Good0.java"), GOOD_SNIPPET);
        Files.writeString(candidate.resolve("fixtures/holdout/Holdout0.java"), BAD_SNIPPET.replace("Bad0", "Holdout0"));
        Files.writeString(candidate.resolve("fixtures/near-miss/NearMiss0.java"),
                GOOD_SNIPPET.replace("Good0", "NearMiss0").replace("int ok = 1;", "System.getSecurityManager();"));
        Files.writeString(candidate.resolve("manifest.json"), "{}\n");
    }
}
