package io.ruleops.factory.candidate;

import com.fasterxml.jackson.databind.ObjectMapper;
import io.ruleops.authoring.AuthoringValidator;
import io.ruleops.authoring.model.RuleIntent;
import io.ruleops.authoring.model.RuleIr;
import io.ruleops.factory.pmd.PmdXPathEmitter;
import io.ruleops.factory.symbol.SymbolRuleTemplate;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.TreeMap;

/**
 * Compiles a governed RuleIntent into a {@link Candidate} through the
 * SymbolRule template: validate intent → template to Rule IR → emit PMD XPath
 * artifact → generate fixtures from intent examples → content-address.
 *
 * <p>Compilation is deterministic: the same intent always produces the same
 * candidate digest, which covers the full IR, the artifact and every fixture.
 * Output is a Candidate — never a RuleRelease (ADR-0021: maturity transitions
 * are evidence-governed).
 */
public final class CandidateCompiler {

    private final SymbolRuleTemplate template = new SymbolRuleTemplate();

    /** Compiles and writes the candidate bundle under {@code outputRoot}. */
    public Path compile(RuleIntent intent, Path outputRoot) throws CandidateCompileException {
        Candidate candidate = build(intent);
        Path dir = outputRoot.resolve(intent.metadata().id()).resolve(candidate.digest());
        try {
            for (Map.Entry<String, byte[]> file : candidate.bundleFiles().entrySet()) {
                Path target = dir.resolve(file.getKey());
                Files.createDirectories(target.getParent());
                Files.write(target, file.getValue());
            }
        } catch (java.io.IOException e) {
            throw new CandidateCompileException(
                    new CandidateError("OUTPUT_UNWRITABLE", e.getMessage()));
        }
        return dir;
    }

    /** Assembles the Candidate in memory (no filesystem). */
    public Candidate build(RuleIntent intent) throws CandidateCompileException {
        var errors = AuthoringValidator.validate(
                new io.ruleops.authoring.AuthoringParser.IntentAsset(intent));
        if (!errors.isEmpty()) {
            String detail = errors.stream()
                    .map(e -> e.code() + " at " + e.path() + ": " + e.message())
                    .reduce((a, b) -> a + "; " + b)
                    .orElse("");
            throw new CandidateCompileException(new CandidateError("INTENT_INVALID", detail));
        }
        RuleIr ir;
        try {
            ir = template.compile(intent);
        } catch (SymbolRuleTemplate.TemplateCompileException e) {
            throw new CandidateCompileException(
                    new CandidateError(e.error().code(), e.error().message()));
        }
        for (String param : ir.match().symbol().parameters()) {
            if (!param.matches("[A-Za-z_][A-Za-z0-9_.$\\[\\]]*")) {
                throw new CandidateCompileException(new CandidateError(
                        "TARGET_INVALID", "invalid parameter type: " + param));
            }
        }

        String artifactFile = "ruleset.xml";
        String severity = intent.spec().severity().defaultLevel();
        int priority = "blocking".equals(severity) ? 1 : 4;
        String ruleName = ruleNameFrom(intent.metadata().id());
        String artifactXml = PmdXPathEmitter.emit(ir, ruleName, priority);
        Map<String, byte[]> fixtures = Candidate.fromExamples(intent);

        String digest = sha256Of(ir, artifactFile, artifactXml, fixtures);
        byte[] manifest = manifestJson(
                intent, ir, artifactFile, fixtures, digest, ruleName, priority)
                .getBytes(StandardCharsets.UTF_8);

        return new Candidate(
                intent.metadata().id(),
                intent.metadata().version(),
                "SymbolRule",
                artifactFile,
                artifactXml.getBytes(StandardCharsets.UTF_8),
                fixtures,
                manifest,
                digest);
    }

    private String manifestJson(
            RuleIntent intent,
            RuleIr ir,
            String artifactFile,
            Map<String, byte[]> fixtures,
            String digest,
            String ruleName,
            int priority) {
        Map<String, Object> manifest = new LinkedHashMap<>();
        manifest.put("kind", "erf-candidate");
        manifest.put("intent", Map.of(
                "id", intent.metadata().id(),
                "version", intent.metadata().version()));
        manifest.put("ir", Map.of(
                "kind", ir.kind().yamlName(),
                "language", ir.language()));
        manifest.put("template", "SymbolRule");
        manifest.put("ruleName", ruleName);
        manifest.put("enginePriority", priority);
        manifest.put("artifact", artifactFile);
        manifest.put("fixtureCount", fixtures.size());
        manifest.put("digest", digest);
        try {
            return new ObjectMapper()
                    .writerWithDefaultPrettyPrinter()
                    .writeValueAsString(manifest) + "\n";
        } catch (com.fasterxml.jackson.core.JsonProcessingException e) {
            throw new IllegalStateException("manifest serialization failed", e);
        }
    }

    /** Dotted intent id becomes a deterministic PascalCase rule name. */
    static String ruleNameFrom(String intentId) {
        StringBuilder sb = new StringBuilder("Erf");
        for (String part : intentId.split("[.-]")) {
            if (!part.isEmpty()) {
                sb.append(Character.toUpperCase(part.charAt(0)))
                        .append(part.substring(1));
            }
        }
        return sb.toString();
    }

    private static String sha256Of(
            RuleIr ir, String artifactFile, String artifactXml, Map<String, byte[]> fixtures) {
        MessageDigest md;
        try {
            md = MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
        try {
            // The digest covers the complete typed IR, not just its kind.
            md.update(new ObjectMapper().writer().writeValueAsBytes(ir));
        } catch (com.fasterxml.jackson.core.JsonProcessingException e) {
            throw new IllegalStateException("IR serialization failed", e);
        }
        md.update((byte) 0);
        md.update(artifactFile.getBytes(StandardCharsets.UTF_8));
        md.update((byte) 0);
        md.update(artifactXml.getBytes(StandardCharsets.UTF_8));
        md.update((byte) 0);
        for (Map.Entry<String, byte[]> f : new TreeMap<>(fixtures).entrySet()) {
            md.update(f.getKey().getBytes(StandardCharsets.UTF_8));
            md.update(f.getValue());
        }
        StringBuilder hex = new StringBuilder();
        for (byte b : md.digest()) {
            hex.append(String.format("%02x", b));
        }
        return hex.toString();
    }
}
