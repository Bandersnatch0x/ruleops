package io.ruleops.factory.pmd;

import io.ruleops.authoring.model.RuleIr;
import io.ruleops.authoring.model.RuleIrKind;
import java.util.List;

/**
 * Emits a PMD 7.x XPath ruleset XML from a compiled SymbolRule IR node.
 *
 * <p>The artifact is a declarative XML ruleset the Backend Host (#8) loads
 * into PMD; ADR-0032 keeps the *runtime* split at result parsing — emitting
 * the compiled artifact is the factory's compile() step of the Backend
 * Adapter SPI (spec §11). The emitted XPath uses PMD 7's type-resolution
 * function so the match is anchored on the declared owner type, the method
 * name and the argument count.
 */
public final class PmdXPathEmitter {

    private PmdXPathEmitter() {}

    /**
     * @param ruleName deterministic engine rule name derived from the intent id
     * @param priority engine-internal PMD priority mapped from intent severity
     *     (blocking=1, advisory=4); policy severity itself is decided by the
     *     runtime from the RuleIntent, never by the engine
     */
    public static String emit(RuleIr ir, String ruleName, int priority) {
        RuleIr.Symbol symbol = ir.match().symbol();
        if (ir.kind() == RuleIrKind.EXISTING_RULE) {
            return emitExisting(symbol, ruleName);
        }
        if (ir.kind() != RuleIrKind.SYMBOL_RULE && ir.kind() != RuleIrKind.STRUCTURAL_RULE) {
            throw new IllegalArgumentException("unsupported Phase 1 IR kind: " + ir.kind());
        }
        // Inside CDATA no XML escaping applies; the XPath string literals are
        // guarded by doubling single quotes (XPath 3.1 escaping), and the
        // compiler validates owner/name/parameters to FQN-safe shapes so
        // "]]>" can never terminate the CDATA section early.
        String owner = xpathLiteral(symbol.owner());
        String name = xpathLiteral(symbol.name());
        List<String> paramsLiteral = symbol.parameters().stream()
                .map(PmdXPathEmitter::xpathLiteral)
                .toList();
        // PMD 7: pmd-java:matchesSig anchors the call on its declaring type,
        // method name and exact parameter list through type resolution — the
        // SymbolRule semantics (owner + name + parameters).
        String signature = owner + "#" + name + "(" + String.join(",", paramsLiteral) + ")";
        String xpath = ir.kind() == RuleIrKind.STRUCTURAL_RULE
                ? structuralXpath(symbol)
                : String.format("//MethodCall[pmd-java:matchesSig('%s')]", signature);
        String message = ir.kind() == RuleIrKind.STRUCTURAL_RULE
                ? "forbidden structural pattern " + symbol.owner() + "." + symbol.name()
                : "forbidden call to " + symbol.owner() + "." + symbol.name();
        StringBuilder sb = new StringBuilder();
        sb.append("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        sb.append("<ruleset name=\"erf-candidate\"\n");
        sb.append("         xmlns=\"http://pmd.sourceforge.net/ruleset/2.0.0\"\n");
        sb.append("         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n");
        sb.append("         xsi:schemaLocation=\"http://pmd.sourceforge.net/ruleset/2.0.0")
                .append(" https://pmd.sourceforge.io/ruleset_2_0_0.xsd\">\n");
        sb.append("  <description>ERF candidate ruleset for ")
                .append(xmlEscape(ruleName))
                .append("</description>\n");
        sb.append("  <rule name=\"").append(xmlEscape(ruleName)).append("\"\n");
        sb.append("        language=\"java\"\n");
        sb.append("        message=\"")
                .append(xmlEscape(message))
                .append("\"\n");
        sb.append("        class=\"net.sourceforge.pmd.lang.rule.xpath.XPathRule\">\n");
        sb.append("    <description>ERF candidate compiled from SymbolRule IR (template artifact)</description>\n");
        sb.append("    <priority>").append(priority).append("</priority>\n");
        sb.append("    <properties>\n");
        // PMD 7 evaluates XPath 3.1 by default (Saxon); no version property.
        sb.append("      <property name=\"xpath\">\n");
        sb.append("        <value><![CDATA[").append(xpath).append("]]></value>\n");
        sb.append("      </property>\n");
        sb.append("    </properties>\n");
        sb.append("  </rule>\n");
        sb.append("</ruleset>\n");
        return sb.toString();
    }

    private static String emitExisting(RuleIr.Symbol symbol, String ruleName) {
        StringBuilder sb = new StringBuilder();
        sb.append("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        sb.append("<ruleset name=\"erf-existing-candidate\" xmlns=\"http://pmd.sourceforge.net/ruleset/2.0.0\">\n");
        sb.append("  <description>ERF ExistingRule candidate bound to PMD identity</description>\n");
        String ref = symbol.name().startsWith("category/")
                ? symbol.name()
                : "category/java/" + symbol.name();
        sb.append("  <rule ref=\"").append(xmlEscape(ref)).append("\" name=\"")
                .append(xmlEscape(ruleName)).append("\">\n");
        for (String parameter : symbol.parameters()) {
            String[] named = parameter.split("=", 2);
            sb.append("    <property name=\"").append(xmlEscape(named[0])).append("\" value=\"")
                    .append(xmlEscape(named[1])).append("\"/>\n");
        }
        sb.append("  </rule>\n</ruleset>\n");
        return sb.toString();
    }

    private static String structuralXpath(RuleIr.Symbol symbol) {
        String node = symbol.owner();
        String attribute = symbol.name();
        if (symbol.parameters().isEmpty()) return "//" + node + "[@" + attribute + "]";
        return "//" + node + "[@" + attribute + "='" + xpathLiteral(symbol.parameters().get(0)) + "']";
    }

    /**
     * Renders an XPath 3.1 string literal body; the caller supplies the
     * surrounding single quotes. A single quote inside the value is escaped
     * by doubling.
     */
    static String xpathLiteral(String value) {
        return value.replace("'", "''");
    }

    static String xmlEscape(String s) {
        return s.replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;")
                .replace("\"", "&quot;")
                .replace("'", "&apos;");
    }
}
