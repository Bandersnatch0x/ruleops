package io.ruleops.authoring;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import io.ruleops.authoring.AuthoringParser.Asset;
import io.ruleops.authoring.AuthoringParser.AuthoringParseException;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Set;
import java.util.stream.Stream;
import org.junit.jupiter.api.Test;

/** Each bad fixture must fail with exactly the expected structured error codes. */
class BadFixtureTest {

    private static final Path BAD = Path.of("src/test/resources/bad");

    private static String read(String name) {
        try {
            return Files.readString(BAD.resolve(name));
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    private static List<String> codes(String fixture) throws AuthoringParseException {
        try {
            Asset asset = AuthoringParser.parse(read(fixture));
            return AuthoringValidator.validate(asset).stream()
                    .map(ValidationError::code)
                    .toList();
        } catch (AuthoringParser.AuthoringParseException e) {
            return List.of(e.code);
        }
    }

    @Test
    void executableScriptFieldIsRejectedByTheClosedSchema() throws Exception {
        // The RuleIntent schema defines no script field; the closed-world binding
        // is what makes the format unable to embed executable scripts.
        assertEquals(List.of("SCHEMA_MALFORMED"), codes("intent-unknown-field.yaml"));
    }

    @Test
    void badScenarioExpectedFailsWithEnumInvalid() throws Exception {
        Set<String> found = Set.copyOf(codes("standard-bad-expected.yaml"));
        assertTrue(found.contains("ENUM_INVALID"), () -> "got: " + found);
    }

    @Test
    void badStandardIdFailsWithFormatInvalid() throws Exception {
        Set<String> found = Set.copyOf(codes("standard-bad-id.yaml"));
        assertTrue(found.contains("FORMAT_INVALID"), () -> "got: " + found);
    }

    @Test
    void unsupportedApiVersionFails() throws Exception {
        Set<String> found = Set.copyOf(codes("intent-bad-api-version.yaml"));
        assertTrue(found.contains("API_VERSION_UNSUPPORTED"), () -> "got: " + found);
    }

    @Test
    void badSeverityFailsWithEnumInvalid() throws Exception {
        Set<String> found = Set.copyOf(codes("intent-bad-severity.yaml"));
        assertTrue(found.contains("ENUM_INVALID"), () -> "got: " + found);
    }

    @Test
    void unknownIrNodeFailsWithEnumInvalid() throws Exception {
        Set<String> found = Set.copyOf(codes("ir-bad-node.yaml"));
        assertTrue(found.contains("ENUM_INVALID"), () -> "got: " + found);
    }

    @Test
    void crossAssetValidationResolvesScenarioReferences() throws Exception {
        Asset standard = AuthoringParser.parse(
                Files.readString(Path.of("..","..","specs/standard/SEC-JAVA-017.yaml")));
        Asset intent = AuthoringParser.parse(read("intent-cross-ref-missing-scenario.yaml"));
        List<ValidationError> errors = AuthoringValidator.validateAll(List.of(standard, intent));
        assertTrue(errors.stream().anyMatch(e -> e.code().equals("CROSS_REF_UNRESOLVED")),
                () -> "got: " + errors);
    }
}
