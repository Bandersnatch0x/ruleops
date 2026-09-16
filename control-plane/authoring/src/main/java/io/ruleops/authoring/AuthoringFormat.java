package io.ruleops.authoring;

/**
 * The governed authoring formats this capability will own. Enumeration only —
 * schema semantics land in ticket #4 — so downstream modules can already name
 * the formats they depend on without reaching into implementation.
 */
public enum AuthoringFormat {
    STANDARD,
    RULE_INTENT,
    RULE_IR
}
