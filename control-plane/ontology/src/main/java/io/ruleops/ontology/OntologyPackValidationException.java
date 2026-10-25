package io.ruleops.ontology;

/** Structured failure emitted by the deterministic pack wire validator. */
public final class OntologyPackValidationException extends Exception {
    private final String code;

    public OntologyPackValidationException(String code, String message) {
        super(message);
        this.code = code;
    }

    public OntologyPackValidationException(String code, String message, Throwable cause) {
        super(message, cause);
        this.code = code;
    }

    public String code() {
        return code;
    }
}
