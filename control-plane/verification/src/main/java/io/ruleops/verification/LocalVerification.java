package io.ruleops.verification;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.stream.Stream;
import net.sourceforge.pmd.PMDConfiguration;
import net.sourceforge.pmd.PmdAnalysis;
import net.sourceforge.pmd.lang.LanguageRegistry;
import net.sourceforge.pmd.lang.rule.RuleSet;
import net.sourceforge.pmd.lang.rule.RuleSetLoader;
import net.sourceforge.pmd.reporting.Report;
import net.sourceforge.pmd.reporting.RuleViolation;

/**
 * Local verification of a Candidate against its own fixtures: run the compiled
 * PMD artifact over the generated bad/good fixtures and assert that every bad
 * fixture is hit and every good fixture is clean. Good/bad fixture assertions
 * are exactly the semantics of the Verification Lab's local layer (ticket #7).
 *
 * <p>Verification runs the ruleset through PMD's programmatic API in-process;
 * this is the author's local loop, distinct from the Backend Host execution
 * path (#8) and never authoritative (ADR-0085).
 */
public final class LocalVerification {

    /** Runs the candidate ruleset over the fixture tree; returns the bundle. */
    public EvidenceBundle verify(Path candidateDir) throws IOException {
        Path rulesetPath = candidateDir.resolve("ruleset.xml");
        if (!Files.isRegularFile(rulesetPath)) {
            throw new IOException("candidate ruleset missing: " + rulesetPath);
        }
        Path fixtures = candidateDir.resolve("fixtures");
        String candidateId = candidateDir.getParent() == null
                ? candidateDir.toString()
                : candidateDir.getParent().getFileName().toString();
        String candidateDigest = candidateDir.getFileName().toString();

        List<Path> javaFiles;
        try (Stream<Path> walk = Files.walk(fixtures)) {
            javaFiles = walk
                    .filter(p -> p.toString().endsWith(".java"))
                    .sorted(Comparator.naturalOrder())
                    .toList();
        }

        PMDConfiguration config = new PMDConfiguration();
        config.setDefaultLanguageVersion(
                LanguageRegistry.PMD.getLanguageById("java").getDefaultVersion());
        config.addInputPath(fixtures);
        // RuleSetLoader.loadFromString(filename, rulesetXmlContent)
        RuleSet ruleset;
        try {
            ruleset = new RuleSetLoader().loadFromString(
                    rulesetPath.getFileName().toString(), Files.readString(rulesetPath));
        } catch (Exception e) {
            StringBuilder chain = new StringBuilder();
            for (Throwable c = e; c != null; c = c.getCause()) {
                chain.append(c.getMessage()).append(" | ");
            }
            throw new IOException("ruleset load failed: " + chain, e);
        }
        List<RuleViolation> violations = new ArrayList<>();
        try (PmdAnalysis pmd = PmdAnalysis.create(config)) {
            pmd.addRuleSet(ruleset);
            Report report = pmd.performAnalysisAndCollectReport();
            violations.addAll(report.getViolations());
        }

        List<EvidenceBundle.FixtureResult> results = new ArrayList<>();
        for (Path file : javaFiles) {
            String relative = fixtures.relativize(file).toString().replace('\\', '/');
            String absolute = file.toAbsolutePath().normalize().toString();
            boolean isBad = relative.startsWith("bad/");
            int expectedHits = isBad ? 1 : 0;
            long actualHits = violations.stream()
                    .map(v -> v.getFileId().getAbsolutePath())
                    .filter(p -> p.equals(absolute) || p.endsWith(relative))
                    .distinct()
                    .count();
            boolean pass = isBad ? actualHits >= expectedHits : actualHits == 0;
            results.add(new EvidenceBundle.FixtureResult(
                    relative,
                    isBad ? "must-hit" : "must-be-clean",
                    expectedHits,
                    (int) actualHits,
                    pass));
        }
        boolean allPass = results.stream().allMatch(EvidenceBundle.FixtureResult::pass);

        return new EvidenceBundle(
                "erf-evidence-bundle",
                EvidenceBundle.TRUST_LEVEL,
                candidateDigest,
                candidateId,
                "pmd-java",
                net.sourceforge.pmd.PMDVersion.VERSION,
                allPass ? "PASS" : "FAIL",
                results,
                allPass
                        ? "all bad fixtures hit, all good fixtures clean"
                        : "fixture expectations violated; see per-fixture results");
    }
}
