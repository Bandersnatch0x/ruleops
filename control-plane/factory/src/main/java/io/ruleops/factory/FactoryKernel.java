package io.ruleops.factory;

import io.ruleops.authoring.AuthoringFormat;

/**
 * Names the authoring formats the Factory Kernel consumes. Implementation of
 * actual compilation lands in ticket #6; the seam keeps the dependency
 * direction explicit from day one.
 */
public final class FactoryKernel {

    private FactoryKernel() {}

    /** True when the kernel can consume the given authoring format as input. */
    public static boolean consumes(AuthoringFormat format) {
        return format == AuthoringFormat.RULE_INTENT || format == AuthoringFormat.RULE_IR;
    }
}
