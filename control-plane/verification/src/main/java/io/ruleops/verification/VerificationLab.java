package io.ruleops.verification;

import io.ruleops.factory.FactoryKernel;

/**
 * Entry seam of the Verification Lab. Implementation lands in ticket #7; the
 * seam proves verification reaches factory output through the module boundary.
 */
public final class VerificationLab {

    private VerificationLab() {}

    /** True when the lab's input pipeline accepts what the factory kernel consumes. */
    public static boolean acceptsFactoryOutput() {
        return FactoryKernel.consumes(io.ruleops.authoring.AuthoringFormat.RULE_INTENT);
    }
}
