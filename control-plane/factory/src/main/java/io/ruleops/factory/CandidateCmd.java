package io.ruleops.factory;

import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.dataformat.yaml.YAMLFactory;
import io.ruleops.authoring.AuthoringValidator;
import io.ruleops.authoring.model.RuleIntent;
import io.ruleops.factory.candidate.CandidateCompileException;
import io.ruleops.factory.candidate.CandidateCompiler;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/**
 * Compile a governed RuleIntent into a Candidate bundle (factory CLI entry).
 * Exit codes: 0 compiled, 1 compile/validation failure, 2 usage error.
 *
 * <p>Usage: java -cp &lt;classpath&gt; io.ruleops.factory.CandidateCmd INTENT.yaml OUTPUT_DIR
 */
public final class CandidateCmd {

    private static final ObjectMapper YAML = new ObjectMapper(new YAMLFactory())
            .enable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES);

    public static void main(String[] args) throws Exception {
        if (args.length != 2) {
            System.err.println("usage: CandidateCmd INTENT.yaml OUTPUT_DIR");
            System.exit(2);
        }
        Path intentPath = Path.of(args[0]);
        if (!Files.isRegularFile(intentPath)) {
            System.err.println("ERROR INPUT_NOT_FOUND: " + intentPath);
            System.exit(2);
        }
        RuleIntent intent;
        try {
            intent = YAML.readValue(Files.readString(intentPath), RuleIntent.class);
        } catch (IOException e) {
            System.err.println("ERROR SCHEMA_MALFORMED: " + e.getMessage());
            System.exit(1);
            return;
        }
        var errors = AuthoringValidator.validate(
                new io.ruleops.authoring.AuthoringParser.IntentAsset(intent));
        if (!errors.isEmpty()) {
            errors.forEach(e -> System.err.println("ERROR " + e));
            System.exit(1);
            return;
        }
        try {
            Path dir = new CandidateCompiler().compile(intent, Path.of(args[1]));
            System.out.println("CANDIDATE " + dir);
        } catch (CandidateCompileException e) {
            System.err.println("ERROR " + e.error().code() + ": " + e.error().message());
            System.exit(1);
        }
    }

    private CandidateCmd() {}
}
