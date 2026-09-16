package io.ruleops.authoring;

import io.ruleops.authoring.AuthoringParser.Asset;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * Validate governed authoring assets (Standard / RuleIntent / Rule IR YAML).
 * Exit codes: 0 all valid, 1 validation failures, 2 usage error.
 *
 * <p>Usage: java -cp &lt;classpath&gt; io.ruleops.authoring.ValidateCmd FILE...
 */
public final class ValidateCmd {

    public static void main(String[] args) throws IOException {
        if (args.length == 0) {
            System.err.println("usage: ValidateCmd FILE...");
            System.exit(2);
        }
        List<Asset> assets = new ArrayList<>();
        List<String> failures = new ArrayList<>();
        for (String arg : args) {
            Path path = Path.of(arg);
            if (!Files.isRegularFile(path)) {
                failures.add(arg + " ERROR_INPUT_NOT_FOUND: not a file");
                continue;
            }
            String yaml = Files.readString(path);
            try {
                assets.add(AuthoringParser.parse(yaml));
            } catch (AuthoringParser.AuthoringParseException e) {
                failures.add(arg + " " + e.code + " at " + e.path + ": " + e.getMessage());
            }
        }
        if (!assets.isEmpty()) {
            for (ValidationError error : AuthoringValidator.validateAll(assets)) {
                failures.add(error.toString());
            }
        }
        if (!failures.isEmpty()) {
            failures.forEach(f -> System.err.println("ERROR " + f));
            System.exit(1);
        }
        for (String arg : args) {
            System.out.println("OK " + arg);
        }
    }

    private ValidateCmd() {}
}
