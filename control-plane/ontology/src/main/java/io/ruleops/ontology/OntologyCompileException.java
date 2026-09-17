package io.ruleops.ontology;

public final class OntologyCompileException extends Exception {
    private final String code;
    public OntologyCompileException(String code, String message) { super(message); this.code = code; }
    public String code() { return code; }
}
