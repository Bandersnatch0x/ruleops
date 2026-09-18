package io.ruleops.verification;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.LinkedHashMap;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.stream.Stream;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import net.sourceforge.pmd.PMDConfiguration;
import net.sourceforge.pmd.PmdAnalysis;
import net.sourceforge.pmd.lang.LanguageRegistry;
import net.sourceforge.pmd.lang.rule.RuleSet;
import net.sourceforge.pmd.lang.rule.RuleSetLoader;
import net.sourceforge.pmd.reporting.Report;
import net.sourceforge.pmd.reporting.RuleViolation;

/**
 * Local verification of a Candidate against its own fixtures: run the compiled
 * PMD artifact over the generated fixture classes and assert that every bad or
 * holdout fixture is hit and every good or near-miss fixture is clean. These
 * fixture assertions are exactly the semantics of the Verification Lab's
 * local layer (ticket #7).
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
        // CandidateCompiler names the bundle directory by content digest.
        String candidateDigest = candidateDir.getFileName().toString();
        verifyContentDigest(candidateDir, candidateDigest);

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
            String category = categoryOf(relative);
            boolean mustHit = category.equals("bad") || category.equals("holdout");
            int expectedHits = mustHit ? 1 : 0;
            long actualHits = violations.stream()
                    .map(v -> v.getFileId().getAbsolutePath())
                    .filter(p -> p.equals(absolute) || p.endsWith(relative))
                    .distinct()
                    .count();
            boolean pass = mustHit ? actualHits >= expectedHits : actualHits == 0;
            results.add(new EvidenceBundle.FixtureResult(
                    relative,
                    category,
                    mustHit ? "must-hit" : "must-be-clean",
                    expectedHits,
                    (int) actualHits,
                    pass));
        }
        boolean allPass = results.stream().allMatch(EvidenceBundle.FixtureResult::pass);

        Map<String, String> environment = Map.of(
                "javaVersion", System.getProperty("java.version"),
                "osName", System.getProperty("os.name"),
                "osArch", System.getProperty("os.arch"));

        return new EvidenceBundle(
                "erf-evidence-bundle",
                EvidenceBundle.TRUST_LEVEL,
                candidateDigest,
                candidateId,
                "pmd-java",
                net.sourceforge.pmd.PMDVersion.VERSION,
                allPass ? "PASS" : "FAIL",
                results,
                environment,
                allPass
                        ? "all bad fixtures hit, all good fixtures clean"
                        : "fixture expectations violated; see per-fixture results");
    }

    private static String categoryOf(String relative) throws IOException {
        int slash = relative.indexOf('/');
        String category = slash > 0 ? relative.substring(0, slash) : "unknown";
        if (!category.equals("bad") && !category.equals("good")
                && !category.equals("holdout") && !category.equals("near-miss")) {
            throw new IOException("unknown fixture category: " + category);
        }
        return category;
    }

    /** Validate content addressing when a compiled manifest carries a digest. */
    private static void verifyContentDigest(Path candidateDir, String directoryDigest) throws IOException {
        Path manifest = candidateDir.resolve("manifest.json");
        if (!Files.isRegularFile(manifest)) throw new IOException("candidate manifest missing: " + manifest);
        JsonNode node = new ObjectMapper().readTree(Files.readString(manifest));
        JsonNode declared = node.get("digest");
        if (declared == null || !declared.isTextual() || declared.asText().isBlank()) return;
        Map<String, byte[]> files = new LinkedHashMap<>();
        files.put("ruleset.xml", Files.readAllBytes(candidateDir.resolve("ruleset.xml")));
        Path fixtures = candidateDir.resolve("fixtures");
        try (Stream<Path> walk = Files.walk(fixtures)) {
            walk.filter(Files::isRegularFile).sorted(Comparator.naturalOrder()).forEach(path -> {
                try {
                    String relative = fixtures.relativize(path).toString().replace('\\', '/');
                    files.put("fixtures/" + relative, Files.readAllBytes(path));
                }
                catch (IOException e) { throw new DigestReadException(e); }
            });
        } catch (DigestReadException e) { throw e.ioException; }
        MessageDigest digest;
        try { digest = MessageDigest.getInstance("SHA-256"); }
        catch (Exception e) { throw new IOException("SHA-256 unavailable", e); }
        files.entrySet().stream().sorted(Map.Entry.comparingByKey()).forEach(entry -> {
            byte[] path = entry.getKey().getBytes(StandardCharsets.UTF_8);
            digest.update(ByteBuffer.allocate(Long.BYTES).putLong(path.length).array());
            digest.update(path);
            digest.update(ByteBuffer.allocate(Long.BYTES).putLong(entry.getValue().length).array());
            digest.update(entry.getValue());
        });
        String actual = hex(digest.digest());
        if (!actual.equals(declared.asText()) || !directoryDigest.equals(declared.asText()))
            throw new IOException("candidate digest mismatch: declared=" + declared.asText() + ", actual=" + actual + ", directory=" + directoryDigest);
    }

    private static String hex(byte[] bytes) {
        StringBuilder out = new StringBuilder(bytes.length * 2);
        for (byte value : bytes) out.append(String.format("%02x", value));
        return out.toString();
    }

    private static final class DigestReadException extends RuntimeException {
        private final IOException ioException;
        DigestReadException(IOException cause) { this.ioException = cause; }
    }
}
