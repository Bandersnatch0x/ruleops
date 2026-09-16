package io.ruleops.authoring;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class AuthoringFormatTest {

    @Test
    void declaresTheThreeGovernedFormats() {
        assertEquals(3, AuthoringFormat.values().length);
    }
}
