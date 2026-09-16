package io.ruleops.factory.pmd;

import io.ruleops.authoring.model.RuleIr;
import io.ruleops.authoring.model.RuleIrKind;

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
        if (ir.kind() != RuleIrKind.SYMBOL_RULE) {
            throw new IllegalArgumentException("emitter handles SymbolRule only, got: " + ir.kind());
        }
        RuleIr.Symbol symbol = ir.match().symbol();
        // Inside CDATA no XML escaping applies; the XPath string literals are
        // guarded by doubling single quotes (XPath 3.1 escaping), and the
        // compiler validates owner/name/parameters to FQN-safe shapes so
        // "]]>" can never terminate the CDATA section early.
        String owner = xpathLiteral(symbol.owner());
        String name = xpathLiteral(symbol.name());
        int argCount = symbol.parameters().size();
        // PMD 7: MethodCall's type is the declaring type of the invoked method,
        // so typeIs pins the owner; ArgumentCount pins the parameter arity.
        String xpath = String.format(
                "//MethodCall[pmd-java:typeIs('%s') and @MethodName='%s' and @ArgumentCount=%d]",
                owner, name, argCount);
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
                .append(xmlEscape("forbidden call to " + symbol.owner() + "." + symbol.name()))
                .append("\"\n");
        sb.append("        class=\"net.sourceforge.pmd.lang.rule.XPathRule\">\n");
        sb.append("    <description>ERF candidate compiled from SymbolRule IR (template artifact)</description>\n");
        sb.append("    <priority>").append(priority).append("</priority>\n");
        sb.append("    <properties>\n");
        sb.append("      <property name=\"xpath\">\n");
        sb.append("        <value><![CDATA[").append(xpath).append("]]></value>\n");
        sb.append("      </property>\n");
        sb.append("      <property name=\"version\" value=\"3.1\"/>\n");
        sb.append("    </properties>\n");
        sb.append("  </rule>\n");
        sb.append("</ruleset>\n");
        return sb.toString();
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
