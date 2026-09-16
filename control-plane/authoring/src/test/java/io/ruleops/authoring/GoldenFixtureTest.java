package io.ruleops.authoring;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import io.ruleops.authoring.AuthoringParser.Asset;
import io.ruleops.authoring.AuthoringParser.AuthoringParseException;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Golden-fixture validation against the spec §7/§9/§10 examples. The canonical
 * fixtures live in specs/ (single authoritative home); tests resolve them via
 * the module's repo-relative path.
 */
class GoldenFixtureTest {

    private static final Path SPECS = Path.of("..", "..", "specs");

    private static String read(Path path) throws IOException {
        return Files.readString(SPECS.resolve(path));
    }

    @Test
    void standardFixtureParsesAndValidates() throws Exception {
        Asset asset = AuthoringParser.parse(read(Path.of("standard/SEC-JAVA-017.yaml")));
        assertTrue(asset instanceof AuthoringParser.StandardAsset);
        assertTrue(AuthoringValidator.validate(asset).isEmpty(),
                () -> "unexpected errors: " + AuthoringValidator.validate(asset));
    }

    @Test
    void ruleIntentFixtureParsesAndValidates() throws Exception {
        Asset asset = AuthoringParser.parse(read(Path.of("rule-intent/corp.java.no-system-exit.yaml")));
        assertTrue(asset instanceof AuthoringParser.IntentAsset);
        assertTrue(AuthoringValidator.validate(asset).isEmpty(),
                () -> "unexpected errors: " + AuthoringValidator.validate(asset));
    }

    @Test
    void ruleIrFixtureParsesAndValidates() throws Exception {
        Asset asset = AuthoringParser.parse(read(Path.of("rule-ir/no-system-exit-symbol-rule.yaml")));
        assertTrue(asset instanceof AuthoringParser.IrAsset);
        assertTrue(AuthoringValidator.validate(asset).isEmpty(),
                () -> "unexpected errors: " + AuthoringValidator.validate(asset));
    }

    @Test
    void intentResolvesAgainstItsStandardAcrossAssets() throws Exception {
        List<Asset> assets = List.of(
                AuthoringParser.parse(read(Path.of("standard/SEC-JAVA-017.yaml"))),
                AuthoringParser.parse(read(Path.of("rule-intent/corp.java.no-system-exit.yaml"))));
        assertEquals(List.of(), AuthoringValidator.validateAll(assets));
    }
}
