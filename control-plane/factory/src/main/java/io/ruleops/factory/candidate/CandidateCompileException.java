package io.ruleops.factory.candidate;

/** Thrown when an intent cannot be compiled into a Candidate. */
public final class CandidateCompileException extends Exception {
    private final CandidateError error;

    public CandidateCompileException(CandidateError error) {
        super(error.code() + ": " + error.message());
        this.error = error;
    }

    public CandidateError error() {
        return error;
    }
}
