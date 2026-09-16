package io.ruleops.factory.pmd;

import io.ruleops.authoring.model.RuleIr;
import io.ruleops.authoring.model.RuleIrKind;
import java.util.List;

/**
 * Emits a PMD 7.x XPath ruleset XML from a compiled SymbolRule IR node.
 *
 * <p>The factory stays engine-free here (ADR-0032): the artifact is a plain
 * XML ruleset the Backend Host (#8) loads into PMD. The emitted XPath uses
 * PMD 7's type-resolution function so the match is anchored on the declared
 * owner type, the method name and the argument count.
 */
public final class PmdXPathEmitter {

    /** Severity of the originating intent, mapped deterministically to PMD priority. */
    private static final List<String> REFERENCE_PRIORITY_COMMENT = List.of(
            "priority mapping: blocking=1, advisory=4 (engine-internal only; policy",
            "severity is decided by the runtime from the RuleIntent)");

    private PmdXPathEmitter() {}

    public static String emit(RuleIr ir) {
        if (ir.kind() != RuleIrKind.SYMBOL_RULE) {
            throw new IllegalArgumentException("emitter handles SymbolRule only, got: " + ir.kind());
        }
        RuleIr.Symbol symbol = ir.match().symbol();
        String owner = xmlEscape(symbol.owner());
        String name = xmlEscape(symbol.name());
        int argCount = symbol.parameters().size();
        // PMD 7: MethodCall's type is the declaring type of the invoked method,
        // so typeIs pins the owner; ArgumentCount pins the parameter arity.
        String xpath = String.format(
                "//MethodCall[pmd-java:typeIs('%s') and @MethodName='%s' and @ArgumentCount=%d]",
                owner, name, argCount);

        String ruleName = ruleNameFrom(ir);
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
        for (String line : REFERENCE_PRIORITY_COMMENT) {
            sb.append("  <!-- ").append(line).append(" -->\n");
        }
        sb.append("  <rule name=\"").append(xmlEscape(ruleName)).append("\"\n");
        sb.append("        language=\"java\"\n");
        sb.append("        message=\"")
                .append(xmlEscape("forbidden call to " + symbol.owner() + "." + symbol.name()))
                .append("\"\n");
        sb.append("        class=\"net.sourceforge.pmd.lang.rule.XPathRule\">\n");
        sb.append("    <description>ERF candidate compiled from SymbolRule IR (template artifact)</description>\n");
        sb.append("    <priority>4</priority>\n");
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

    /** Deterministic rule name derived from the IR kind ("ErfSymbolRule"). */
    static String ruleNameFrom(RuleIr ir) {
        return "Erf" + ir.kind().yamlName();
    }

    static String xmlEscape(String s) {
        return s.replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;")
                .replace("\"", "&quot;")
                .replace("'", "&apos;");
    }
}
