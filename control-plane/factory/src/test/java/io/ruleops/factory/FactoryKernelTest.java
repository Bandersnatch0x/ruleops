package io.ruleops.factory;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import io.ruleops.authoring.AuthoringFormat;
import org.junit.jupiter.api.Test;

class FactoryKernelTest {

    @Test
    void consumesIntentAndIrButNotStandards() {
        assertTrue(FactoryKernel.consumes(AuthoringFormat.RULE_INTENT));
        assertTrue(FactoryKernel.consumes(AuthoringFormat.RULE_IR));
        assertFalse(FactoryKernel.consumes(AuthoringFormat.STANDARD));
    }
}
