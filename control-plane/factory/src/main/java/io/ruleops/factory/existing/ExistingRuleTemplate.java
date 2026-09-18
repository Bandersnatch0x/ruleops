package io.ruleops.factory.existing;

import io.ruleops.authoring.model.RuleIntent;
import io.ruleops.authoring.model.RuleIr;
import io.ruleops.authoring.model.RuleIrKind;
import java.util.List;
import java.util.Set;

/**
 * Template for an existing PMD rule. The PMD rule id and parameter strings are
 * carried into IR verbatim; this template never silently substitutes a new
 * rule or changes parameter semantics.
 */
public final class ExistingRuleTemplate {
    public static final Set<String> PROVIDED_CAPABILITIES = Set.of("java.ast");
    private static final Set<String> SUPPORTED_ENGINES = Set.of("pmd");

    public record TemplateError(String code, String message) {}
    public static final class TemplateCompileException extends Exception {
        private final TemplateError error;
        TemplateCompileException(TemplateError error) { super(error.code() + ": " + error.message()); this.error = error; }
        public TemplateError error() { return error; }
    }

    public RuleIr compile(RuleIntent intent) throws TemplateCompileException {
        RuleIntent.Target target = intent.spec().semantics().target();
        if (!"existing-rule".equals(target.kind())) throw new TemplateCompileException(new TemplateError("TARGET_KIND_UNSUPPORTED", "ExistingRule template requires target.kind=existing-rule"));
        if (!SUPPORTED_ENGINES.contains(target.declaringType())) throw new TemplateCompileException(new TemplateError("ENGINE_UNSUPPORTED", "ExistingRule supports only pmd, got: " + target.declaringType()));
        if (target.method() == null || !target.method().matches("[A-Za-z][A-Za-z0-9_.-]*(/[A-Za-z][A-Za-z0-9_.-]*)*")) throw new TemplateCompileException(new TemplateError("RULE_ID_INVALID", "invalid PMD rule identity: " + target.method()));
        if (target.parameters().stream().anyMatch(p -> !p.matches("[A-Za-z][A-Za-z0-9_.-]*=[A-Za-z0-9_.:+/-]+")))
            throw new TemplateCompileException(new TemplateError("PARAMETER_INVALID", "ExistingRule parameters must be named key=value"));
        if (!intent.spec().requiredCapabilities().stream().allMatch(PROVIDED_CAPABILITIES::contains)) throw new TemplateCompileException(new TemplateError("CAPABILITY_GAP", "ExistingRule requires only java.ast"));
        return new RuleIr(intent.apiVersion(), RuleIrKind.EXISTING_RULE, intent.spec().language().id(),
                new RuleIr.Match("existing-rule", new RuleIr.Symbol(target.declaringType(), target.method(), List.copyOf(target.parameters()))),
                new RuleIr.Scope(List.copyOf(intent.spec().scope().includeSourceSets())), intent.spec().unresolved().behavior());
    }
}
