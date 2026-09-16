package io.ruleops.authoring;

/**
 * A single structured validation failure with a stable machine code, the
 * document path it applies to, and a human explanation. Codes are part of the
 * authoring contract: callers branch on code, never on message text.
 */
public record ValidationError(String code, String path, String message) {

    @Override
    public String toString() {
        return code + " at " + path + ": " + message;
    }
}
