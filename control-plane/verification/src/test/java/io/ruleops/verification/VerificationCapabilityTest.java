package io.ruleops.verification;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class VerificationCapabilityTest {

    @Test
    void isWiredToTheFactoryCapability() {
        assertEquals("factory", VerificationCapability.upstreamCapability());
    }
}
