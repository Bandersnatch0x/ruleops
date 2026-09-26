package io.ruleops.javahost;

import erf.backend.v0.AnalysisUnit;
import erf.backend.v0.BackendObservation;
import erf.backend.v0.ObservationLocation;
import erf.backend.v0.PlannedUnit;
import erf.backend.v0.SourceSpan;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.InvalidPathException;
import java.nio.file.LinkOption;
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
final class PmdBackendAdapter implements BackendAdapter {

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
        Path normalizedRoot = trustedRepoRoot(repoRoot);
        Path normalizedRuleset = trustedRulesetFile(rulesetFile);
        RuleSet ruleset = new RuleSetLoader().loadFromString(
                normalizedRuleset.getFileName().toString(), Files.readString(normalizedRuleset));
        return new PmdBackendAdapter(normalizedRoot, ruleset, normalizedRuleset.getFileName().toString());
    }

    /** Describe-only adapter for sessions launched before a ruleset exists. */
    static PmdBackendAdapter empty() {
        return new PmdBackendAdapter(null, null, null);
    }

    @Override
    public String engineVersion() {
        return net.sourceforge.pmd.PMDVersion.VERSION;
    }

    @Override
    public String backendId() {
        return "pmd-java";
    }

    @Override
    public java.util.Map<String, Boolean> capabilityFacts() {
        return java.util.Map.of("java.ast", true, "java.symbol-resolution", true);
    }

    @Override
    public java.util.List<String> ruleKinds() {
        return java.util.List.of("SymbolRule");
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
    public List<BackendObservation> observe(AnalysisUnit unit) throws IOException {
        if (ruleset == null) {
            throw new IOException("host launched without ERF_RULESET; cannot execute units");
        }
        JavaExecutionContext context = JavaExecutionContext.from(repoRoot, unit);
        List<Path> files = new ArrayList<>();
        for (String relative : unit.getFilesList()) {
            files.add(resolveRepositoryFile(relative));
        }
        List<BackendObservation> observations = new ArrayList<>();
        if (files.isEmpty()) {
            return observations;
        }
        PMDConfiguration config = new PMDConfiguration();
        List<Path> classpath = context.classpathAndBuildOutputs();
        if (!classpath.isEmpty()) {
            config.prependAuxClasspath(
                    String.join(
                            java.io.File.pathSeparator,
                            classpath.stream().map(Path::toString).toList()));
        }
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
        final Path absolute;
        try {
            absolute = Path.of(absolutePath).toAbsolutePath().normalize();
        } catch (InvalidPathException error) {
            throw new IOException("PMD returned an invalid observation path", error);
        }
        if (repoRoot == null || !absolute.startsWith(repoRoot)
                || Files.isSymbolicLink(absolute)
                || !Files.isRegularFile(absolute, LinkOption.NOFOLLOW_LINKS)) {
            throw new IOException("PMD returned an observation outside the repository root: " + absolutePath);
        }
        Path cursor = repoRoot;
        for (Path component : repoRoot.relativize(absolute)) {
            cursor = cursor.resolve(component);
            if (Files.isSymbolicLink(cursor)) {
                throw new IOException("PMD returned an observation through a symbolic link: " + absolutePath);
            }
        }
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

    private static Path trustedRepoRoot(Path candidate) throws IOException {
        if (candidate == null || Files.isSymbolicLink(candidate)) {
            throw new IOException("repository root must be a real directory");
        }
        Path root = candidate.toAbsolutePath().normalize();
        if (!Files.isDirectory(root, LinkOption.NOFOLLOW_LINKS)) {
            throw new IOException("repository root is not a real directory: " + candidate);
        }
        Path cursor = root.getRoot();
        for (Path component : root) {
            cursor = cursor.resolve(component);
            if (Files.isSymbolicLink(cursor)) {
                throw new IOException("repository root traverses a symbolic link: " + candidate);
            }
        }
        return root;
    }

    private static Path trustedRulesetFile(Path candidate) throws IOException {
        if (candidate == null) {
            throw new IOException("ruleset file is missing");
        }
        Path file = candidate.toAbsolutePath().normalize();
        if (Files.isSymbolicLink(file)
                || !Files.isRegularFile(file, LinkOption.NOFOLLOW_LINKS)) {
            throw new IOException("ruleset file must be a real regular file: " + candidate);
        }
        Path cursor = file.getRoot();
        for (Path component : file) {
            cursor = cursor.resolve(component);
            if (Files.isSymbolicLink(cursor)) {
                throw new IOException("ruleset path traverses a symbolic link: " + candidate);
            }
        }
        return file;
    }

    private Path resolveRepositoryFile(String relative) throws IOException {
        final Path relativePath;
        try {
            relativePath = Path.of(relative);
        } catch (InvalidPathException error) {
            throw new IOException("unit file path is invalid: " + relative, error);
        }
        if (relativePath.isAbsolute() || relativePath.normalize().startsWith(Path.of(".."))) {
            throw new IOException("unit file path is not repository-relative: " + relative);
        }
        Path absolute = repoRoot.resolve(relativePath).normalize();
        if (!absolute.startsWith(repoRoot) || Files.isSymbolicLink(absolute)
                || !Files.isRegularFile(absolute, LinkOption.NOFOLLOW_LINKS)) {
            throw new IOException("unit file missing or linked outside repo root: " + relative);
        }
        Path cursor = repoRoot;
        for (Path component : repoRoot.relativize(absolute)) {
            cursor = cursor.resolve(component);
            if (Files.isSymbolicLink(cursor)) {
                throw new IOException("unit file traverses a symbolic link: " + relative);
            }
        }
        return absolute;
    }
}
