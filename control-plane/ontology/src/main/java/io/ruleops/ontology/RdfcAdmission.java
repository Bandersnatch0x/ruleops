package io.ruleops.ontology;

/**
 * Fail-closed production admission boundary.
 *
 * <p>Pack validation is useful evidence, but it is not a signature verifier or
 * an RDFC-1.0 implementation.  Until the remaining ADR-0181 gates exist this
 * boundary must never return an admitted result.</p>
 */
public final class RdfcAdmission {
    private final OntologyPackValidator validator;

    public RdfcAdmission() {
        this(new OntologyPackValidator());
    }

    RdfcAdmission(OntologyPackValidator validator) {
        this.validator = validator;
    }

    public AdmissionDecision evaluate(OntologyPackEnvelope pack) {
        OntologyPackValidator.ValidationResult validation = validator.validate(pack);
        return new AdmissionDecision(false, RdfcAdmissionStatus.RDFC_NOT_ADMITTED, validation);
    }

    public record AdmissionDecision(
            boolean admitted,
            RdfcAdmissionStatus status,
            OntologyPackValidator.ValidationResult validation) {
    }
}
