package io.ruleops.factory;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import io.ruleops.authoring.AuthoringParser;
import io.ruleops.authoring.model.RuleIntent;
import io.ruleops.authoring.model.RuleIr;
import io.ruleops.factory.candidate.CandidateCompileException;
import io.ruleops.factory.candidate.CandidateCompiler;
import io.ruleops.factory.symbol.SymbolRuleTemplate;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/**
 * Candidate compilation: the spec's no-system-exit intent must compile
 * deterministically into a Candidate bundle; intents outside the template's
 * applicability must fail explicitly.
 */
class CandidateCompilerTest {

    private static String goldenIntent() throws IOException {
        return Files.readString(Path.of("..", "..", "specs/rule-intent/corp.java.no-system-exit.yaml"));
    }

    private static RuleIntent parse(String yaml) throws Exception {
        return ((AuthoringParser.IntentAsset) AuthoringParser.parse(yaml)).document();
    }

    @Test
    void templateCompilesStaticMethodCallToSymbolRule() throws Exception {
        RuleIr ir = new SymbolRuleTemplate().compile(parse(goldenIntent()));
        assertEquals(io.ruleops.authoring.model.RuleIrKind.SYMBOL_RULE, ir.kind());
        assertEquals("java.lang.System", ir.match().symbol().owner());
        assertEquals("exit", ir.match().symbol().name());
        assertEquals(java.util.List.of("int"), ir.match().symbol().parameters());
        assertEquals("unknown", ir.onUnresolved());
    }

    @Test
    void templateRejectsVirtualInvocationExplicitly() throws Exception {
        String yaml = goldenIntent().replace("invocation: static", "invocation: virtual");
        SymbolRuleTemplate.TemplateCompileException e = assertThrows(
                SymbolRuleTemplate.TemplateCompileException.class,
                () -> new SymbolRuleTemplate().compile(parse(yaml)));
        assertEquals("INVOCATION_UNSUPPORTED", e.error().code());
    }

    @Test
    void templateRejectsUnsatisfiedCapabilitiesExplicitly() throws Exception {
        String yaml = goldenIntent().replace(
                "requiredCapabilities:\n    - java.ast\n    - java.symbol-resolution",
                "requiredCapabilities:\n    - java.bytecode");
        SymbolRuleTemplate.TemplateCompileException e = assertThrows(
                SymbolRuleTemplate.TemplateCompileException.class,
                () -> new SymbolRuleTemplate().compile(parse(yaml)));
        assertEquals("CAPABILITY_GAP", e.error().code());
        assertTrue(e.error().message().contains("java.bytecode"));
    }

    @Test
    void candidateBundleIsCompleteAndContentAddressed(@TempDir Path out) throws Exception {
        Path dir = new CandidateCompiler().compile(parse(goldenIntent()), out);
        assertTrue(Files.exists(dir.resolve("ruleset.xml")));
        assertTrue(Files.exists(dir.resolve("manifest.json")));
        assertTrue(Files.exists(dir.resolve("fixtures/bad/Bad0.java")));
        assertTrue(Files.exists(dir.resolve("fixtures/good/Good0.java")));
        String manifest = Files.readString(dir.resolve("manifest.json"));
        assertTrue(manifest.contains("digest"),
                "directory name must be the digest");
    }

    @Test
    void sameIntentCompilesToIdenticalDigest(@TempDir Path out) throws Exception {
        RuleIntent intent = parse(goldenIntent());
        Path first = new CandidateCompiler().compile(intent, out);
        Path second = new CandidateCompiler().compile(intent, out);
        assertEquals(first.getFileName().toString(), second.getFileName().toString());
        assertEquals(
                Files.readString(first.resolve("ruleset.xml")),
                Files.readString(second.resolve("ruleset.xml")));
    }

    @Test
    void emittedRulesetPinsOwnerNameAndArity(@TempDir Path out) throws Exception {
        Path dir = new CandidateCompiler().compile(parse(goldenIntent()), out);
        String xml = Files.readString(dir.resolve("ruleset.xml"));
        assertTrue(xml.contains("pmd-java:matchesSig('java.lang.System#exit(int)')"), () -> xml);
    }

    @Test
    void severityMapsToEnginePriorityAndRuleNameIsDerivedFromIntent() throws Exception {
        RuleIntent intent = parse(goldenIntent());
        var candidate = new CandidateCompiler().build(intent);
        String xml = new String(candidate.artifact(), java.nio.charset.StandardCharsets.UTF_8);
        // advisory -> 4
        assertTrue(xml.contains("<priority>4</priority>"), () -> xml);
        assertTrue(xml.contains("rule name=\"ErfCorpJavaNoSystemExit\""), () -> xml);
    }

    @Test
    void existingRuleBindsPmdIdentityAndParameters(@TempDir Path out) throws Exception {
        String yaml = goldenIntent()
                .replace("kind: method-call", "kind: existing-rule")
                .replace("declaringType: java.lang.System", "declaringType: pmd")
                .replace("method: exit", "method: UnusedPrivateMethod")
                .replace("parameters: [int]", "parameters: [priority=3]")
                .replace("- java.symbol-resolution\n", "");
        Path dir = new CandidateCompiler().compile(parse(yaml), out);
        String xml = Files.readString(dir.resolve("ruleset.xml"));
        assertTrue(xml.contains("ref=\"category/java/UnusedPrivateMethod\""), () -> xml);
        assertTrue(xml.contains("name=\"priority\" value=\"3\""), () -> xml);
        assertTrue(Files.readString(dir.resolve("manifest.json")).contains("\"template\" : \"ExistingRule\""));
    }

    @Test
    void structuralRuleCompilesOnlyWithinDeclaredBoundary(@TempDir Path out) throws Exception {
        String yaml = goldenIntent()
                .replace("kind: method-call", "kind: structural-node")
                .replace("declaringType: java.lang.System", "declaringType: MethodDeclaration")
                .replace("method: exit", "method: Name")
                .replace("parameters: [int]", "parameters: [Forbidden]")
                .replace("- java.symbol-resolution\n", "");
        Path dir = new CandidateCompiler().compile(parse(yaml), out);
        assertTrue(Files.readString(dir.resolve("ruleset.xml"))
                .contains("//MethodDeclaration[@Name='Forbidden']"));
        assertTrue(Files.readString(dir.resolve("manifest.json")).contains("\"template\" : \"StructuralRule\""));
    }

    @Test
    void structuralRuleRejectsUnboundedNodeShape() throws Exception {
        String yaml = goldenIntent()
                .replace("kind: method-call", "kind: structural-node")
                .replace("declaringType: java.lang.System", "declaringType: LambdaExpression")
                .replace("method: exit", "method: Name")
                .replace("parameters: [int]", "parameters: []")
                .replace("- java.symbol-resolution\n", "");
        CandidateCompileException error = assertThrows(CandidateCompileException.class,
                () -> new CandidateCompiler().build(parse(yaml)));
        assertEquals("STRUCTURE_UNSUPPORTED", error.error().code());
    }

}
