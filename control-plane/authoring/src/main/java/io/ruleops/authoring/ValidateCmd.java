package io.ruleops.authoring;

import io.ruleops.authoring.AuthoringParser.Asset;
import io.ruleops.authoring.AuthoringParser.IntentAsset;
import io.ruleops.authoring.AuthoringParser.StandardAsset;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Validate governed authoring assets (Standard / RuleIntent / Rule IR YAML).
 * Every failure is reported with the file it belongs to. Exit codes: 0 all
 * valid, 1 validation failures, 2 usage error.
 *
 * <p>Usage: java -cp &lt;classpath&gt; io.ruleops.authoring.ValidateCmd FILE...
 */
public final class ValidateCmd {

    public static void main(String[] args) throws IOException {
        if (args.length == 0) {
            System.err.println("usage: ValidateCmd FILE...");
            System.exit(2);
        }
        Map<Asset, String> assets = new LinkedHashMap<>();
        List<String> failures = new ArrayList<>();
        for (String arg : args) {
            Path path = Path.of(arg);
            if (!Files.isRegularFile(path)) {
                failures.add(arg + " ERROR_INPUT_NOT_FOUND: not a file");
                continue;
            }
            String yaml = Files.readString(path);
            try {
                assets.put(AuthoringParser.parse(yaml), arg);
            } catch (AuthoringParser.AuthoringParseException e) {
                failures.add(arg + " " + e.code + " at " + e.path + ": " + e.getMessage());
            }
        }
        for (Map.Entry<Asset, String> entry : assets.entrySet()) {
            for (ValidationError error : AuthoringValidator.validate(entry.getKey())) {
                failures.add(entry.getValue() + " " + error);
            }
        }
        List<Asset> standards = assets.keySet().stream()
                .filter(a -> a instanceof StandardAsset)
                .toList();
        for (Map.Entry<Asset, String> entry : assets.entrySet()) {
            if (entry.getKey() instanceof IntentAsset intent) {
                List<Asset> scope = new ArrayList<>();
                scope.add(intent);
                scope.addAll(standards);
                for (ValidationError error : AuthoringValidator.validateCrossRefs(scope)) {
                    failures.add(entry.getValue() + " " + error);
                }
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
