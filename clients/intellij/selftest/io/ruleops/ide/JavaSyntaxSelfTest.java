package io.ruleops.ide;

import com.sun.source.util.JavacTask;
import javax.tools.*;
import java.nio.file.*;
import java.util.*;

/** Parse-only evidence when no IntelliJ SDK exists. This deliberately does not claim type checking. */
public final class JavaSyntaxSelfTest {
    public static void main(String[] args) throws Exception {
        var compiler = ToolProvider.getSystemJavaCompiler();
        var diagnostics = new DiagnosticCollector<JavaFileObject>();
        try (var manager = compiler.getStandardFileManager(diagnostics, null, null);
             var paths = Files.walk(Path.of("clients/intellij/src"))) {
            var files = paths.filter(p -> p.toString().endsWith(".java")).toList();
            var units = manager.getJavaFileObjectsFromPaths(files);
            var task = (JavacTask) compiler.getTask(null, manager, diagnostics, List.of("-proc:none"), null, units);
            for (var ignored : task.parse()) { }
            var errors = diagnostics.getDiagnostics().stream().filter(d -> d.getKind() == Diagnostic.Kind.ERROR).toList();
            if (!errors.isEmpty()) throw new AssertionError(errors.toString());
            System.out.println("intellij-java-syntax: parsed=" + files.size() + " errors=0 (NOT SDK type checking)");
        }
    }
}
