package io.ruleops.verification;

import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class VerificationLabTest {

    @Test
    void labPipelineReachesFactoryOutput() {
        assertTrue(VerificationLab.acceptsFactoryOutput());
    }
}
