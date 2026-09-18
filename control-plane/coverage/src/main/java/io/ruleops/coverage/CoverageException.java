package io.ruleops.coverage;

/** Structured rejection of an incomplete or contradictory Coverage input. */
public final class CoverageException extends Exception {
    private final String code;
    private final String path;

    public CoverageException(String code, String path, String message) {
        super(message);
        this.code = code;
        this.path = path;
    }

    public String code() {
        return code;
    }

    public String path() {
        return path;
    }
}
