package io.ruleops.authoring;

import com.fasterxml.jackson.core.JacksonException;
import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.dataformat.yaml.YAMLFactory;
import io.ruleops.authoring.model.RuleIntent;
import io.ruleops.authoring.model.RuleIr;
import io.ruleops.authoring.model.RuleIrKind;
import io.ruleops.authoring.model.Standard;

/**
 * Closed-world YAML binding for governed authoring assets. Unknown fields fail
 * parsing (closed field sets are what make the RuleIntent format unable to
 * express executable scripts), as do trailing tokens and bad types.
 */
public final class AuthoringParser {

    public static final String API_VERSION = "rulefactory.io/v1";

    private static final ObjectMapper STRICT_MAPPER = new ObjectMapper(new YAMLFactory())
            .enable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES)
            .enable(DeserializationFeature.FAIL_ON_TRAILING_TOKENS);

    // The probe reads only `kind`; strictness is enforced by STRICT_MAPPER on the full binding.
    private static final ObjectMapper PROBE_MAPPER = new ObjectMapper(new YAMLFactory())
            .disable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES)
            .enable(DeserializationFeature.FAIL_ON_TRAILING_TOKENS);

    /** Parsed asset of one of the three governed kinds, with the source it came from. */
    public sealed interface Asset permits StandardAsset, IntentAsset, IrAsset {}

    public record StandardAsset(Standard document) implements Asset {}

    public record IntentAsset(RuleIntent document) implements Asset {}

    public record IrAsset(RuleIr document) implements Asset {}

    /** Parse one YAML document into the asset type its `kind` declares. */
    public static Asset parse(String yaml) throws AuthoringParseException {
        KindProbe probe;
        try {
            probe = PROBE_MAPPER.readValue(yaml, KindProbe.class);
        } catch (JacksonException e) {
            throw new AuthoringParseException("SCHEMA_MALFORMED", "$", e.getMessage(), e);
        }
        if (probe == null || probe.kind == null) {
            throw new AuthoringParseException("KIND_MISSING", "$", "document has no kind field");
        }
        try {
            return switch (probe.kind) {
                case "Standard" -> new StandardAsset(STRICT_MAPPER.readValue(yaml, Standard.class));
                case "RuleIntent" -> new IntentAsset(STRICT_MAPPER.readValue(yaml, RuleIntent.class));
                default -> {
                    if (RuleIrKind.fromYamlName(probe.kind) != null) {
                        yield new IrAsset(STRICT_MAPPER.readValue(yaml, RuleIr.class));
                    }
                    throw new AuthoringParseException(
                            "KIND_UNKNOWN", "$.kind", "unknown kind: " + probe.kind);
                }
            };
        } catch (JacksonException e) {
            throw new AuthoringParseException("SCHEMA_MALFORMED", "$", e.getMessage(), e);
        }
    }

    public record KindProbe(String kind) {}

    /** Parse failure carrying a machine code and document path. */
    public static final class AuthoringParseException extends Exception {
        public final String code;
        public final String path;

        AuthoringParseException(String code, String path, String message) {
            super(message);
            this.code = code;
            this.path = path;
        }

        AuthoringParseException(String code, String path, String message, Throwable cause) {
            super(message, cause);
            this.code = code;
            this.path = path;
        }
    }

    private AuthoringParser() {}
}
