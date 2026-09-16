package io.ruleops.javahost;

import erf.backend.v0.AnalysisUnit;
import erf.backend.v0.BackendObservation;
import erf.backend.v0.ObservationLocation;
import erf.backend.v0.PlannedUnit;
import erf.backend.v0.SourceSpan;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import net.sourceforge.pmd.PMDConfiguration;
import net.sourceforge.pmd.PmdAnalysis;
import net.sourceforge.pmd.lang.LanguageRegistry;
import net.sourceforge.pmd.lang.rule.RuleSet;
import net.sourceforge.pmd.lang.rule.RuleSetLoader;
import net.sourceforge.pmd.reporting.Report;
import net.sourceforge.pmd.reporting.RuleViolation;

/**
 * The Backend Adapter SPI seam hosting PMD 7.x (spec §11): describe reports
 * the engine's capability facts, plan refines assigned units into
 * deterministic batches without touching the Completeness denominator
 * (ADR-0036), execute runs the ruleset per unit and maps violations to typed
 * BackendObservations. Policy severity, Gate and Baseline decisions are
 * never made here.
 */
final class PmdBackendAdapter {

    private final Path repoRoot;
    private final RuleSet ruleset;
    private final String rulesetName;

    private PmdBackendAdapter(Path repoRoot, RuleSet ruleset, String rulesetName) {
        this.repoRoot = repoRoot;
        this.ruleset = ruleset;
        this.rulesetName = rulesetName;
    }

    /** Loads the adapter from the trusted locations the runtime configured. */
    static PmdBackendAdapter load(Path repoRoot, Path rulesetFile) throws IOException {
        RuleSet ruleset = new RuleSetLoader().loadFromString(
                rulesetFile.getFileName().toString(), Files.readString(rulesetFile));
        return new PmdBackendAdapter(repoRoot, ruleset, rulesetFile.getFileName().toString());
    }

    /** Describe-only adapter for sessions launched before a ruleset exists. */
    static PmdBackendAdapter empty() {
        return new PmdBackendAdapter(null, null, null);
    }

    String engineVersion() {
        return net.sourceforge.pmd.PMDVersion.VERSION;
    }

    /** Deterministic per-unit batch refinement; the denominator is frozen. */
    List<PlannedUnit> plan(List<AnalysisUnit> units) {
        List<PlannedUnit> planned = new ArrayList<>();
        for (AnalysisUnit unit : units) {
            planned.add(PlannedUnit.newBuilder()
                    .setUnitId(unit.getUnitId())
                    .setBatchId("batch-0")
                    .build());
        }
        return planned;
    }

    /**
     * Runs the ruleset over one unit's files and maps each violation to a
     * typed, policy-neutral BackendObservation (engine severity stays
     * diagnostic; the runtime owns policy).
     */
    List<BackendObservation> observe(AnalysisUnit unit) throws IOException {
        if (ruleset == null) {
            throw new IOException("host launched without ERF_RULESET; cannot execute units");
        }
        List<Path> files = new ArrayList<>();
        for (String relative : unit.getFilesList()) {
            Path absolute = repoRoot.resolve(relative).normalize();
            if (!Files.isRegularFile(absolute)) {
                throw new IOException("unit file missing under repo root: " + relative);
            }
            files.add(absolute);
        }
        List<BackendObservation> observations = new ArrayList<>();
        if (files.isEmpty()) {
            return observations;
        }
        PMDConfiguration config = new PMDConfiguration();
        config.setDefaultLanguageVersion(
                LanguageRegistry.PMD.getLanguageById(unit.getLanguage()).getDefaultVersion());
        try (PmdAnalysis pmd = PmdAnalysis.create(config)) {
            pmd.addRuleSet(ruleset);
            for (Path file : files) {
                pmd.files().addFile(file);
            }
            Report report = pmd.performAnalysisAndCollectReport();
            for (RuleViolation violation : report.getViolations()) {
                observations.add(toObservation(violation));
            }
        }
        return observations;
    }

    private BackendObservation toObservation(RuleViolation violation) throws IOException {
        String absolutePath = violation.getFileId().getAbsolutePath();
        Path absolute = Path.of(absolutePath).toAbsolutePath().normalize();
        String relative = repoRoot.relativize(absolute).toString().replace('\\', '/');
        SourceSpans.Span span = SourceSpans.forViolation(
                absolute, violation.getBeginLine(), violation.getBeginColumn());
        return BackendObservation.newBuilder()
                .setRuleId(violation.getRule().getName())
                .setLocation(ObservationLocation.newBuilder()
                        .setSourceSpan(SourceSpan.newBuilder()
                                .setPath(relative)
                                .setBlobSha256(
                                        com.google.protobuf.ByteString.copyFrom(span.blobSha256()))
                                .setStartByte(span.startByte())
                                .setEndByte(span.endByte())
                                .setLine(span.line())
                                .setColumn(span.column())))
                .setMessage(String.valueOf(violation.getDescription()))
                .setEngineSeverity(String.valueOf(violation.getRule().getPriority()))
                .build();
    }
}
