package io.ruleops.factory.candidate;

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
import java.util.StringJoiner;
import java.util.Map;

/**
 * Compiles a governed RuleIntent into a {@link Candidate} through the
 * SymbolRule template: validate intent → template to Rule IR → emit PMD XPath
 * artifact → generate fixtures from intent examples → content-address.
 *
 * <p>Compilation is deterministic: the same intent bytes always produce the
 * same candidate digest. Output is a Candidate — never a RuleRelease (ADR-0021:
 * maturity transitions are evidence-governed).
 */
public final class CandidateCompiler {

    private final SymbolRuleTemplate template = new SymbolRuleTemplate();

    /** Compiles and writes the candidate bundle under {@code outputRoot}. */
    public Path compile(RuleIntent intent, Path outputRoot) throws CandidateCompileException {
        var errors = AuthoringValidator.validate(
                new io.ruleops.authoring.AuthoringParser.IntentAsset(intent));
        if (!errors.isEmpty()) {
            throw new CandidateCompileException(new CandidateError(
                    "INTENT_INVALID", errors.toString()));
        }
        RuleIr ir;
        try {
            ir = template.compile(intent);
        } catch (SymbolRuleTemplate.TemplateCompileException e) {
            throw new CandidateCompileException(
                    new CandidateError(e.error().code(), e.error().message()));
        }
        String artifactXml = PmdXPathEmitter.emit(ir);
        Map<String, byte[]> fixtures = Fixtures.fromExamples(intent);

        String artifactFile = "ruleset.xml";
        String digest = sha256Of(ir, artifactXml, fixtures);
        String manifest = manifestJson(intent, ir, artifactFile, fixtures, digest);

        Map<String, byte[]> bundle = new LinkedHashMap<>();
        bundle.put(artifactFile, artifactXml.getBytes(StandardCharsets.UTF_8));
        bundle.putAll(fixtures);
        bundle.put(Candidate.MANIFEST_FILE, manifest.getBytes(StandardCharsets.UTF_8));

        Path dir = outputRoot.resolve(intent.metadata().id()).resolve(digest);
        try {
            for (Map.Entry<String, byte[]> file : bundle.entrySet()) {
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

    private String manifestJson(
            RuleIntent intent,
            RuleIr ir,
            String artifactFile,
            Map<String, byte[]> fixtures,
            String digest) {
        StringJoiner j = new StringJoiner(",\n");
        j.add("  \"kind\": \"erf-candidate\"");
        j.add("  \"intent\": {\"id\": " + json(intent.metadata().id())
                + ", \"version\": " + json(intent.metadata().version()) + "}");
        j.add("  \"ir\": {\"kind\": " + json(ir.kind().yamlName())
                + ", \"language\": " + json(ir.language()) + "}");
        j.add("  \"template\": \"SymbolRule\"");
        j.add("  \"artifact\": " + json(artifactFile));
        j.add("  \"fixtureCount\": " + fixtures.size());
        j.add("  \"digest\": " + json(digest));
        return "{\n" + j + "\n}\n";
    }

    private static String sha256Of(RuleIr ir, String artifactXml, Map<String, byte[]> fixtures) {
        MessageDigest md;
        try {
            md = MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
        md.update(ir.kind().yamlName().getBytes(StandardCharsets.UTF_8));
        md.update((byte) 0);
        md.update(artifactXml.getBytes(StandardCharsets.UTF_8));
        md.update((byte) 0);
        for (Map.Entry<String, byte[]> f : new java.util.TreeMap<>(fixtures).entrySet()) {
            md.update(f.getKey().getBytes(StandardCharsets.UTF_8));
            md.update(f.getValue());
        }
        StringBuilder hex = new StringBuilder();
        for (byte b : md.digest()) {
            hex.append(String.format("%02x", b));
        }
        return hex.toString();
    }

    private static String json(String s) {
        return "\"" + s.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }
}
