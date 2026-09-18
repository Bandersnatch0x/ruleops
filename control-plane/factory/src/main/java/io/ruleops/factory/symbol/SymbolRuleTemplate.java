package io.ruleops.factory.symbol;

import io.ruleops.authoring.model.RuleIntent;
import io.ruleops.authoring.model.RuleIr;
import io.ruleops.authoring.model.RuleIrKind;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * The SymbolRule typed template: compiles a governed RuleIntent whose semantic
 * target is a static method call into a SymbolRule IR node (spec §10.1).
 *
 * <p>The template is explicit about what it can implement: a target kind of
 * {@code method-call}, a {@code static} invocation, and required capabilities
 * it actually provides. Anything else fails compilation — no silent
 * degradation to a lower-trust implementation (spec §11: Router must allow
 * UNSUPPORTED).
 */
public final class SymbolRuleTemplate {

    /** Capability facts this template's backend implementation provides. */
    public static final Set<String> PROVIDED_CAPABILITIES =
            Set.of("java.ast", "java.symbol-resolution");

    private static final String TARGET_KIND = "method-call";
    private static final String STATIC_INVOCATION = "static";

    /** Compilation failure with a structured, stable code. */
    public record TemplateError(String code, String message) {}

    /**
     * Compiles the intent's semantics into a SymbolRule IR node.
     *
     * @throws TemplateCompileException when the intent is outside the
     *     template's explicit applicability
     */
    public RuleIr compile(RuleIntent intent) throws TemplateCompileException {
        RuleIntent.Spec spec = intent.spec();
        RuleIntent.Target target = spec.semantics().target();

        if (!TARGET_KIND.equals(target.kind())) {
            throw new TemplateCompileException(new TemplateError(
                    "TARGET_KIND_UNSUPPORTED",
                    "SymbolRule template implements method-call only, got: " + target.kind()));
        }
        if (!STATIC_INVOCATION.equals(target.invocation())) {
            throw new TemplateCompileException(new TemplateError(
                    "INVOCATION_UNSUPPORTED",
                    "SymbolRule template implements static invocation only, got: "
                            + target.invocation()));
        }
        Set<String> missing = new HashSet<>(spec.requiredCapabilities());
        missing.removeAll(PROVIDED_CAPABILITIES);
        if (!missing.isEmpty()) {
            throw new TemplateCompileException(new TemplateError(
                    "CAPABILITY_GAP",
                    "template cannot provide required capabilities: " + missing));
        }

        return new RuleIr(
                intent.apiVersion(),
                RuleIrKind.SYMBOL_RULE,
                spec.language().id(),
                new RuleIr.Match(
                        TARGET_KIND,
                        new RuleIr.Symbol(
                                target.declaringType(),
                                target.method(),
                                List.copyOf(target.parameters()))),
                new RuleIr.Scope(List.copyOf(spec.scope().includeSourceSets())),
                spec.unresolved().behavior());
    }

    /** Thrown when the intent does not fit the template's applicability. */
    public static final class TemplateCompileException extends Exception {
        private final TemplateError error;

        TemplateCompileException(TemplateError error) {
            super(error.code() + ": " + error.message());
            this.error = error;
        }

        public TemplateError error() {
            return error;
        }
    }
}
