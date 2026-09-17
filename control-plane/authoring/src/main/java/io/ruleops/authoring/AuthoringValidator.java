package io.ruleops.authoring;

import io.ruleops.authoring.AuthoringParser.Asset;
import io.ruleops.authoring.AuthoringParser.IntentAsset;
import io.ruleops.authoring.AuthoringParser.IrAsset;
import io.ruleops.authoring.AuthoringParser.StandardAsset;
import io.ruleops.authoring.model.RuleIntent;
import io.ruleops.authoring.model.RuleIr;
import io.ruleops.authoring.model.RuleIrKind;
import io.ruleops.authoring.model.Standard;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * Semantic validation for governed authoring assets on top of the closed-world
 * schema binding. Produces structured {@link ValidationError}s with stable
 * codes. Also performs cross-asset checks when several assets are validated
 * together (an intent's standard/scenario/language references must resolve).
 */
public final class AuthoringValidator {

    private static final Pattern STANDARD_ID = Pattern.compile("^[A-Z][A-Z0-9]*(-[A-Z0-9]+)+$");
    private static final Pattern INTENT_ID = Pattern.compile("^[a-z][a-z0-9-]*(\\.[a-z][a-z0-9-]*)+$");
    private static final Pattern SEMVER = Pattern.compile("^\\d+\\.\\d+\\.\\d+$");
    private static final Pattern SOURCE_LEVEL = Pattern.compile("^\\d+$");
    private static final Pattern SOURCE_SET = Pattern.compile("^[a-z][a-z0-9-]*$");

    private static final Set<String> SCENARIO_EXPECTED =
            Set.of("detectable", "needs-clarification", "unsupported");
    /** Symbol-rule node types; shared by intent semantics targets and IR match nodes. */
    private static final Set<String> NODE_KINDS = Set.of("method-call", "existing-rule", "structural-node");
    private static final Set<String> TARGET_INVOCATION = Set.of("static", "virtual", "any");
    private static final Set<String> UNRESOLVED_BEHAVIOR = Set.of("unknown", "fail", "ignore");
    private static final Set<String> SEVERITY = Set.of("advisory", "blocking");

    /** Validate a single asset in isolation. */
    public static List<ValidationError> validate(Asset asset) {
        return switch (asset) {
            case StandardAsset s -> validateStandard(s.document());
            case IntentAsset i -> validateIntent(i.document());
            case IrAsset ir -> validateIr(ir.document());
        };
    }

    /**
     * Validate a set of assets together: every RuleIntent's standard id+version
     * and scenario id must resolve against a validated Standard, and its
     * language must be in that Standard's languages.
     */
    public static List<ValidationError> validateAll(List<Asset> assets) {
        List<ValidationError> errors = new ArrayList<>();
        Map<String, Standard> standards = new LinkedHashMap<>();
        for (Asset asset : assets) {
            if (asset instanceof StandardAsset s) {
                errors.addAll(validateStandard(s.document()));
                standards.put(s.document().metadata().id(), s.document());
            }
        }
        for (Asset asset : assets) {
            switch (asset) {
                case IntentAsset i -> {
                    errors.addAll(validateIntent(i.document()));
                    errors.addAll(crossRef(i.document(), standards));
                }
                case IrAsset ir -> errors.addAll(validateIr(ir.document()));
                case StandardAsset ignored -> { /* already validated */ }
            }
        }
        return errors;
    }

    /** Cross-asset checks only: every intent's Standard/scenario/language must resolve. */
    public static List<ValidationError> validateCrossRefs(List<Asset> assets) {
        List<ValidationError> errors = new ArrayList<>();
        Map<String, Standard> standards = new LinkedHashMap<>();
        for (Asset asset : assets) {
            if (asset instanceof StandardAsset s) {
                standards.put(s.document().metadata().id(), s.document());
            }
        }
        for (Asset asset : assets) {
            if (asset instanceof IntentAsset i) {
                errors.addAll(crossRef(i.document(), standards));
            }
        }
        return errors;
    }

    private static List<ValidationError> crossRef(
            RuleIntent intent, Map<String, Standard> standards) {
        List<ValidationError> errors = new ArrayList<>();
        RuleIntent.StandardRef ref = intent.spec().standard();
        Standard standard = standards.get(ref.id());
        if (standard == null) {
            errors.add(new ValidationError(
                    "CROSS_REF_UNRESOLVED", "$.spec.standard.id",
                    "no Standard '" + ref.id() + "' among validated assets"));
            return errors;
        }
        if (!standard.metadata().version().equals(ref.version())) {
            errors.add(new ValidationError(
                    "CROSS_REF_VERSION_MISMATCH", "$.spec.standard.version",
                    "intent pins standard version " + ref.version() + " but Standard is "
                            + standard.metadata().version()));
        }
        boolean scenarioFound = standard.spec().scenarios().stream()
                .anyMatch(s -> s.id().equals(ref.scenario()));
        if (!scenarioFound) {
            errors.add(new ValidationError(
                    "CROSS_REF_UNRESOLVED", "$.spec.standard.scenario",
                    "Standard '" + ref.id() + "' declares no scenario '" + ref.scenario() + "'"));
        }
        if (!standard.spec().languages().contains(intent.spec().language().id())) {
            errors.add(new ValidationError(
                    "CROSS_REF_LANGUAGE_MISMATCH", "$.spec.language.id",
                    "Standard '" + ref.id() + "' does not declare language '"
                            + intent.spec().language().id() + "'"));
        }
        return errors;
    }

    private static List<ValidationError> validateStandard(Standard s) {
        List<ValidationError> errors = new ArrayList<>();
        checkApiVersion(errors, s.apiVersion());
        kindMismatch(errors, s.kind(), "Standard");
        id(errors, "$.metadata.id", s.metadata() == null ? null : s.metadata().id(), STANDARD_ID);
        if (s.metadata() != null) {
            version(errors, "$.metadata.version", s.metadata().version());
            required(errors, "$.metadata.owner", s.metadata().owner());
        }
        if (s.spec() == null) {
            required(errors, "$.spec", null);
            return errors;
        }
        required(errors, "$.spec.title", s.spec().title());
        if (s.spec().languages() == null || s.spec().languages().isEmpty()) {
            errors.add(new ValidationError("REQUIRED_MISSING", "$.spec.languages", "at least one language required"));
        }
        if (s.spec().scenarios() == null || s.spec().scenarios().isEmpty()) {
            errors.add(new ValidationError("REQUIRED_MISSING", "$.spec.scenarios", "at least one scenario required"));
            return errors;
        }
        Set<String> scenarioIds = new HashSet<>();
        for (int i = 0; i < s.spec().scenarios().size(); i++) {
            Standard.Scenario sc = s.spec().scenarios().get(i);
            String base = "$.spec.scenarios[" + i + "]";
            if (sc.id() == null || sc.id().isBlank()) {
                errors.add(new ValidationError("REQUIRED_MISSING", base + ".id", "scenario id required"));
            } else if (!scenarioIds.add(sc.id())) {
                errors.add(new ValidationError("DUPLICATE", base + ".id", "duplicate scenario id: " + sc.id()));
            }
            checkEnum(errors, base + ".expected", sc.expected(), SCENARIO_EXPECTED);
        }
        return errors;
    }

    private static List<ValidationError> validateIntent(RuleIntent r) {
        List<ValidationError> errors = new ArrayList<>();
        checkApiVersion(errors, r.apiVersion());
        kindMismatch(errors, r.kind(), "RuleIntent");
        if (r.metadata() != null) {
            id(errors, "$.metadata.id", r.metadata().id(), INTENT_ID);
            version(errors, "$.metadata.version", r.metadata().version());
        }
        RuleIntent.Spec spec = r.spec();
        if (spec == null) {
            required(errors, "$.spec", null);
            return errors;
        }
        if (spec.standard() == null) {
            required(errors, "$.spec.standard", null);
        } else {
            id(errors, "$.spec.standard.id", spec.standard().id(), STANDARD_ID);
            version(errors, "$.spec.standard.version", spec.standard().version());
            required(errors, "$.spec.standard.scenario", spec.standard().scenario());
        }
        if (spec.language() == null) {
            required(errors, "$.spec.language", null);
        } else {
            required(errors, "$.spec.language.id", spec.language().id());
            if (spec.language().sourceLevels() == null || spec.language().sourceLevels().isEmpty()) {
                errors.add(new ValidationError(
                        "REQUIRED_MISSING", "$.spec.language.sourceLevels",
                        "at least one source level required"));
            } else {
                for (int i = 0; i < spec.language().sourceLevels().size(); i++) {
                    String level = spec.language().sourceLevels().get(i);
                    if (level == null || !SOURCE_LEVEL.matcher(level).matches()) {
                        errors.add(new ValidationError(
                                "FORMAT_INVALID", "$.spec.language.sourceLevels[" + i + "]",
                                "numeric source level required, got: " + level));
                    }
                }
            }
        }
        if (spec.intent() == null || spec.intent().statement() == null || spec.intent().statement().isBlank()) {
            required(errors, "$.spec.intent.statement", null);
        }
        if (spec.scope() == null) {
            required(errors, "$.spec.scope", null);
        } else {
            checkSourceSets(errors, "$.spec.scope.includeSourceSets", spec.scope().includeSourceSets(), true);
            checkSourceSets(errors, "$.spec.scope.excludeSourceSets", spec.scope().excludeSourceSets(), false);
        }
        if (spec.semantics() == null || spec.semantics().target() == null) {
            required(errors, "$.spec.semantics.target", null);
        } else {
            RuleIntent.Target t = spec.semantics().target();
            checkEnum(errors, "$.spec.semantics.target.kind", t.kind(), NODE_KINDS);
            required(errors, "$.spec.semantics.target.declaringType", t.declaringType());
            required(errors, "$.spec.semantics.target.method", t.method());
            checkEnum(errors, "$.spec.semantics.target.invocation", t.invocation(), TARGET_INVOCATION);
        }
        if (spec.requiredCapabilities() == null || spec.requiredCapabilities().isEmpty()) {
            errors.add(new ValidationError(
                    "REQUIRED_MISSING", "$.spec.requiredCapabilities",
                    "at least one required capability must be declared"));
        }
        if (spec.unresolved() == null) {
            errors.add(new ValidationError("REQUIRED_MISSING", "$.spec.unresolved", "required"));
        } else {
            checkEnum(errors, "$.spec.unresolved.behavior", spec.unresolved().behavior(), UNRESOLVED_BEHAVIOR);
        }
        if (spec.examples() == null) {
            required(errors, "$.spec.examples", null);
        } else {
            if (spec.examples().bad() == null || spec.examples().bad().isEmpty()) {
                errors.add(new ValidationError(
                        "REQUIRED_MISSING", "$.spec.examples.bad", "at least one bad example required"));
            }
            if (spec.examples().good() == null || spec.examples().good().isEmpty()) {
                errors.add(new ValidationError(
                        "REQUIRED_MISSING", "$.spec.examples.good", "at least one good example required"));
            }
        }
        if (spec.severity() == null) {
            errors.add(new ValidationError("REQUIRED_MISSING", "$.spec.severity", "required"));
        } else {
            checkEnum(errors, "$.spec.severity.default", spec.severity().defaultLevel(), SEVERITY);
        }
        return errors;
    }

    private static List<ValidationError> validateIr(RuleIr ir) {
        List<ValidationError> errors = new ArrayList<>();
        checkApiVersion(errors, ir.apiVersion());
        if (ir.kind() == null) {
            errors.add(new ValidationError("KIND_MISSING", "$.kind", "IR kind required"));
        }
        required(errors, "$.language", ir.language());
        if (ir.match() == null) {
            required(errors, "$.match", null);
            return errors;
        }
        checkEnum(errors, "$.match.node", ir.match().node(), NODE_KINDS);
        if (ir.match().symbol() == null) {
            required(errors, "$.match.symbol", null);
        } else {
            required(errors, "$.match.symbol.owner", ir.match().symbol().owner());
            required(errors, "$.match.symbol.name", ir.match().symbol().name());
        }
        if (ir.scope() == null) {
            required(errors, "$.scope", null);
        } else {
            checkSourceSets(errors, "$.scope.sourceSets", ir.scope().sourceSets(), true);
        }
        checkEnum(errors, "$.onUnresolved", ir.onUnresolved(), UNRESOLVED_BEHAVIOR);
        return errors;
    }

    private static void checkSourceSets(
            List<ValidationError> errors, String path, List<String> sourceSets, boolean requiredList) {
        if (sourceSets == null) {
            if (requiredList) {
                errors.add(new ValidationError("REQUIRED_MISSING", path, "required"));
            }
            return;
        }
        for (int i = 0; i < sourceSets.size(); i++) {
            String ss = sourceSets.get(i);
            if (ss == null || !SOURCE_SET.matcher(ss).matches()) {
                errors.add(new ValidationError(
                        "FORMAT_INVALID", path + "[" + i + "]", "invalid source set name: " + ss));
            }
        }
    }

    private static void checkApiVersion(List<ValidationError> errors, String apiVersion) {
        if (!AuthoringParser.API_VERSION.equals(apiVersion)) {
            errors.add(new ValidationError(
                    "API_VERSION_UNSUPPORTED", "$.apiVersion",
                    "expected " + AuthoringParser.API_VERSION + ", got: " + apiVersion));
        }
    }

    private static void kindMismatch(List<ValidationError> errors, String kind, String expectedKind) {
        if (!expectedKind.equals(kind)) {
            errors.add(new ValidationError(
                    "KIND_MISMATCH", "$.kind", "expected " + expectedKind + ", got: " + kind));
        }
    }

    private static void checkEnum(
            List<ValidationError> errors, String path, String value, Set<String> allowed) {
        if (value == null || !allowed.contains(value)) {
            errors.add(new ValidationError(
                    "ENUM_INVALID", path, "expected one of " + allowed + ", got: " + value));
        }
    }

    private static void required(List<ValidationError> errors, String path, String value) {
        if (value == null || value.isBlank()) {
            errors.add(new ValidationError("REQUIRED_MISSING", path, "required"));
        }
    }

    private static void id(List<ValidationError> errors, String path, String value, Pattern pattern) {
        if (value == null || !pattern.matcher(value).matches()) {
            errors.add(new ValidationError("FORMAT_INVALID", path, "invalid id: " + value));
        }
    }

    private static void version(List<ValidationError> errors, String path, String value) {
        if (value == null || !SEMVER.matcher(value).matches()) {
            errors.add(new ValidationError("FORMAT_INVALID", path, "semver required, got: " + value));
        }
    }

    private AuthoringValidator() {}
}
