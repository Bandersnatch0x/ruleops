package io.ruleops.javahost;

import erf.backend.v0.AnalysisUnit;
import erf.backend.v0.BackendObservation;
import java.io.IOException;
import java.util.List;
import java.util.Map;

/** Backend-neutral seam for typed Java analysis adapters. */
interface BackendAdapter {

    String backendId();

    String engineVersion();

    Map<String, Boolean> capabilityFacts();

    List<String> ruleKinds();

    List<BackendObservation> observe(AnalysisUnit unit) throws IOException;
}
