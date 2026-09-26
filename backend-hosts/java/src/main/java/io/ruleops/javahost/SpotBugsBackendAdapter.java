package io.ruleops.javahost;

import edu.umd.cs.findbugs.BugCollectionBugReporter;
import edu.umd.cs.findbugs.BugInstance;
import edu.umd.cs.findbugs.BugReporter;
import edu.umd.cs.findbugs.FindBugs2;
import edu.umd.cs.findbugs.DetectorFactoryCollection;
import edu.umd.cs.findbugs.config.UserPreferences;
import edu.umd.cs.findbugs.MethodAnnotation;
import edu.umd.cs.findbugs.Project;
import edu.umd.cs.findbugs.SortedBugCollection;
import edu.umd.cs.findbugs.ClassAnnotation;
import erf.backend.v0.AnalysisUnit;
import erf.backend.v0.BackendObservation;
import erf.backend.v0.BytecodeMember;
import erf.backend.v0.ObservationLocation;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.PrintWriter;
import java.io.PrintStream;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Restricted SpotBugs adapter for compiled Java units. The adapter consumes
 * class files that the Runtime/Workspace contract already supplied; it never
 * invokes Maven, Gradle, javac, or any project-owned process.
 */
final class SpotBugsBackendAdapter implements BackendAdapter {

    private static final String BACKEND_ID = "spotbugs-java";
    // Keep the negotiated capability tied to the dependency version in the
    // checked-in POM. The upstream Version.VERSION_STRING is absent from the
    // shaded release jar and reports "(Unknown)" there.
    private static final String ENGINE_VERSION = "4.9.8";
    private final Path repoRoot;

    private SpotBugsBackendAdapter(Path repoRoot) {
        this.repoRoot = repoRoot.toAbsolutePath().normalize();
    }

    static SpotBugsBackendAdapter load(Path repoRoot) throws IOException {
        if (!Files.isDirectory(repoRoot, LinkOption.NOFOLLOW_LINKS)) {
            throw new IOException("SpotBugs repository root is not a directory: " + repoRoot);
        }
        return new SpotBugsBackendAdapter(repoRoot);
    }

    @Override
    public String backendId() {
        return BACKEND_ID;
    }

    @Override
    public String engineVersion() {
        return ENGINE_VERSION;
    }

    @Override
    public Map<String, Boolean> capabilityFacts() {
        return Map.of("java.bytecode", true, "java.symbol-resolution", true);
    }

    @Override
    public List<String> ruleKinds() {
        return List.of("BytecodeRule");
    }

    @Override
    public List<BackendObservation> observe(AnalysisUnit unit) throws IOException {
        if (!unit.getLanguage().equals("java")) {
            throw new IOException("SpotBugs adapter only accepts java units");
        }
        JavaExecutionContext context = JavaExecutionContext.from(repoRoot, unit);
        List<String> relativeFiles = new ArrayList<>(unit.getFilesList());
        relativeFiles.sort(String::compareTo);
        Set<String> identities = new HashSet<>();
        List<Path> classFiles = new ArrayList<>();
        for (String relative : relativeFiles) {
            if (!identities.add(relative)) {
                throw new IOException("duplicate bytecode file in unit: " + relative);
            }
            Path absolute = resolveRepositoryFile(relative);
            if (!relative.endsWith(".class")) {
                throw new IOException("SpotBugs unit file is not a class file: " + relative);
            }
            if (!Files.isRegularFile(absolute, LinkOption.NOFOLLOW_LINKS)) {
                throw new IOException("SpotBugs class file is missing: " + relative);
            }
            byte[] header = new byte[4];
            int headerBytes;
            try (InputStream input = Files.newInputStream(absolute)) {
                headerBytes = input.readNBytes(header, 0, header.length);
            }
            if (headerBytes != header.length
                    || header[0] != (byte) 0xca
                    || header[1] != (byte) 0xfe
                    || header[2] != (byte) 0xba
                    || header[3] != (byte) 0xbe) {
                throw new IOException("SpotBugs input is not a JVM class file: " + relative);
            }
            classFiles.add(absolute);
        }
        if (classFiles.isEmpty()) {
            throw new IOException("SpotBugs unit contains no class files");
        }

        Project project = new Project();
        project.setProjectName(
                unit.getUnitId()
                        + (context.invocationIdentity().isEmpty()
                                ? ""
                                : "@" + context.invocationIdentity()));
        for (Path entry : context.classpathAndBuildOutputs()) {
            if (!project.addAuxClasspathEntry(entry.toString())) {
                throw new IOException("SpotBugs rejected Java classpath entry: " + entry);
            }
        }
        for (Path classFile : classFiles) {
            if (!project.addFile(classFile.toString())) {
                throw new IOException("SpotBugs rejected class file: " + classFile);
            }
        }

        System.setProperty("findbugs.debug", "false");
        System.setProperty("findbugs.progress", "false");
        PrintStream originalStdout = System.out;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        try (FindBugs2 engine = new FindBugs2()) {
            BugReporter reporter = new BugCollectionBugReporter(
                    project, new PrintWriter(OutputStream.nullOutputStream()));
            reporter.setPriorityThreshold(3);
            engine.setProject(project);
            DetectorFactoryCollection detectors = DetectorFactoryCollection.instance();
            engine.setDetectorFactoryCollection(detectors);
            UserPreferences preferences = UserPreferences.createDefaultUserPreferences();
            preferences.enableAllDetectors(true);
            engine.setUserPreferences(preferences);
            engine.setAnalysisFeatureSettings(preferences.getAnalysisFeatureSettings());
            engine.setBugReporter(reporter);
            engine.setNoClassOk(false);
            engine.finishSettings();
            engine.execute();
            reporter.finish();
            if (engine.getErrorCount() > 0 || engine.getMissingClassCount() > 0) {
                throw new IOException("SpotBugs analysis was incomplete: errors="
                        + engine.getErrorCount() + ", missingClasses="
                        + engine.getMissingClassCount());
            }
            SortedBugCollection collection = (SortedBugCollection) reporter.getBugCollection();
            List<BugInstance> bugs = new ArrayList<>();
            collection.iterator().forEachRemaining(bugs::add);
            bugs.sort(Comparator
                    .comparing(BugInstance::getType)
                    .thenComparingInt(BugInstance::getPriority)
                    .thenComparing(SpotBugsBackendAdapter::primaryClass)
                    .thenComparing(SpotBugsBackendAdapter::primaryMember)
                    .thenComparing(BugInstance::getMessage));
            List<BackendObservation> observations = new ArrayList<>();
            for (BugInstance bug : bugs) {
                observations.add(toObservation(bug));
            }
            return observations;
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new IOException("SpotBugs analysis interrupted", e);
        } finally {
            System.setOut(originalStdout);
        }
    }

    private Path resolveRepositoryFile(String relative) throws IOException {
        Path requested;
        try {
            requested = Path.of(relative);
        } catch (RuntimeException e) {
            throw new IOException("invalid SpotBugs file path: " + relative, e);
        }
        Path candidate = repoRoot.resolve(requested).normalize();
        if (requested.isAbsolute() || relative.startsWith("/")
                || relative.contains("\\") || relative.contains("\0")
                || containsParentSegment(requested)
                || !candidate.startsWith(repoRoot)) {
            throw new IOException("SpotBugs file path escapes repository root: " + relative);
        }
        Path current = repoRoot;
        for (Path segment : requested) {
            current = current.resolve(segment);
            if (Files.isSymbolicLink(current)) {
                throw new IOException("SpotBugs file path crosses a symbolic link: " + relative);
            }
        }
        return candidate;
    }

    private static boolean containsParentSegment(Path path) {
        for (Path segment : path) {
            if (segment.toString().equals("..")) return true;
        }
        return false;
    }

    private static BackendObservation toObservation(BugInstance bug) {
        String nativeId = bug.getType();
        String ruleId = "spotbugs." + nativeId.toLowerCase().replace('_', '-');
        String className = primaryClass(bug);
        String member = primaryMember(bug);
        String descriptor = primaryDescriptor(bug);
        return BackendObservation.newBuilder()
                .setRuleId(ruleId)
                .setLocation(ObservationLocation.newBuilder()
                        .setBytecodeMember(BytecodeMember.newBuilder()
                                .setClassFqn(className)
                                .setMember(member)
                                .setMemberDescriptor(descriptor)))
                .setMessage("[" + nativeId + "] " + bug.getMessage())
                .setEngineSeverity(bug.getPriorityString())
                .build();
    }

    private static String primaryClass(BugInstance bug) {
        ClassAnnotation annotation = bug.getPrimaryClass();
        return annotation == null ? "" : annotation.getClassName();
    }

    private static String primaryMember(BugInstance bug) {
        MethodAnnotation method = bug.getPrimaryMethod();
        return method == null ? "<class>" : method.getMethodName();
    }

    private static String primaryDescriptor(BugInstance bug) {
        MethodAnnotation method = bug.getPrimaryMethod();
        return method == null ? "" : method.getMethodSignature();
    }
}
