package io.ruleops.factory.structural;

import io.ruleops.authoring.model.RuleIntent;
import io.ruleops.authoring.model.RuleIr;
import io.ruleops.authoring.model.RuleIrKind;
import java.util.List;
import java.util.Set;

/**
 * Bounded AST-shape template. V0 deliberately accepts only a node name and a
 * single literal attribute predicate; unsupported structural expressions fail
 * instead of being approximated.
 */
public final class StructuralRuleTemplate {
    private static final Set<String> NODES = Set.of("ClassDeclaration", "MethodDeclaration", "FieldDeclaration");
    public record TemplateError(String code, String message) {}
    public static final class TemplateCompileException extends Exception {
        private final TemplateError error;
        TemplateCompileException(TemplateError error) { super(error.code() + ": " + error.message()); this.error = error; }
        public TemplateError error() { return error; }
    }

    public RuleIr compile(RuleIntent intent) throws TemplateCompileException {
        RuleIntent.Target target = intent.spec().semantics().target();
        if (!"structural-node".equals(target.kind())) throw new TemplateCompileException(new TemplateError("TARGET_KIND_UNSUPPORTED", "StructuralRule template requires target.kind=structural-node"));
        if (!NODES.contains(target.declaringType())) throw new TemplateCompileException(new TemplateError("STRUCTURE_UNSUPPORTED", "supported nodes are " + NODES + ", got: " + target.declaringType()));
        if (target.method() == null || !target.method().matches("[A-Za-z][A-Za-z0-9_-]*")) throw new TemplateCompileException(new TemplateError("ATTRIBUTE_INVALID", "structural attribute must be a simple name"));
        if (target.parameters().size() > 1 || target.parameters().stream().anyMatch(p -> !p.matches("[A-Za-z0-9_.:-]+"))) throw new TemplateCompileException(new TemplateError("STRUCTURE_UNSUPPORTED", "StructuralRule supports at most one literal parameter"));
        if (!intent.spec().requiredCapabilities().contains("java.ast")) throw new TemplateCompileException(new TemplateError("CAPABILITY_GAP", "StructuralRule requires java.ast"));
        return new RuleIr(intent.apiVersion(), RuleIrKind.STRUCTURAL_RULE, intent.spec().language().id(),
                new RuleIr.Match("structural-node", new RuleIr.Symbol(target.declaringType(), target.method(), List.copyOf(target.parameters()))),
                new RuleIr.Scope(List.copyOf(intent.spec().scope().includeSourceSets())), intent.spec().unresolved().behavior());
    }
}
