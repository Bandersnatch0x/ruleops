package io.ruleops.factory;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class FactoryCapabilityTest {

    @Test
    void isWiredToTheAuthoringCapability() {
        assertEquals("authoring", FactoryCapability.upstreamCapability());
    }
}
