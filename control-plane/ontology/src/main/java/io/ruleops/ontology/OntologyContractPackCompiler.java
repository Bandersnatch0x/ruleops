package io.ruleops.ontology;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import org.apache.jena.graph.Graph;
import org.apache.jena.graph.Node;
import org.apache.jena.graph.Triple;
import org.apache.jena.riot.RDFDataMgr;
import org.apache.jena.riot.RDFLanguages;
import org.apache.jena.riot.out.NodeFmtLib;

/** Jena-centered compiler for the small Phase 1 ontology boundary. */
public final class OntologyContractPackCompiler {
    public static final String CORE_NS = "https://ruleops.io/ontology/core#";
    private static final Set<String> EXTERNAL = Set.of(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
            "http://www.w3.org/2000/01/rdf-schema#",
            "http://www.w3.org/2002/07/owl#",
            "http://www.w3.org/2001/XMLSchema#");

    public OntologyContractPack compile(Path core, List<Path> extensions, String revision)
            throws IOException, OntologyCompileException {
        List<String> lines = new ArrayList<>();
        Set<String> declared = new HashSet<>();
        try {
            read(core, lines, declared, false);
            for (Path extension : extensions) read(extension, lines, declared, true);
        } catch (ClosedWorldRuntimeException e) {
            throw new OntologyCompileException("CLOSED_WORLD", "IRI is not declared: " + e.getMessage());
        } catch (IllegalStateException e) {
            throw new OntologyCompileException("CANONICAL_UNSUPPORTED", e.getMessage());
        }
        lines.sort(Comparator.naturalOrder());
        byte[] canonical = String.join("\n", lines).concat("\n").getBytes(StandardCharsets.UTF_8);
        return new OntologyContractPack(revision, OntologyContractPack.sha256(canonical), canonical);
    }

    private void read(Path source, List<String> lines, Set<String> declared, boolean extension)
            throws IOException, OntologyCompileException {
        Graph graph;
        try { graph = RDFDataMgr.loadGraph(source.toString(), RDFLanguages.TURTLE); }
        catch (RuntimeException e) { throw new OntologyCompileException("TURTLE_INVALID", e.getMessage()); }
        Set<String> subjects = new HashSet<>();
        var iterator = graph.find(Node.ANY, Node.ANY, Node.ANY);
        while (iterator.hasNext()) {
            Triple t = iterator.next();
            if (t.getSubject().isURI()) subjects.add(t.getSubject().getURI());
        }
        for (String subject : subjects) {
            if (extension && subject.startsWith(CORE_NS))
                throw new OntologyCompileException("CORE_REDEFINED", "extension defines core IRI: " + subject);
            declared.add(subject);
        }
        iterator = graph.find(Node.ANY, Node.ANY, Node.ANY);
        while (iterator.hasNext()) {
            Triple t = iterator.next();
            if (t.getSubject().isBlank() || t.getPredicate().isBlank() || t.getObject().isBlank())
                throw new IllegalStateException("blank nodes are not supported in canonical v0 pack");
            String object = t.getObject().isURI() ? t.getObject().getURI() : null;
            if (object != null && !EXTERNAL.stream().anyMatch(object::startsWith)
                    && !declared.contains(object) && !subjects.contains(object)) {
                throw new ClosedWorldRuntimeException(object);
            }
            lines.add(NodeFmtLib.strNT(t.getSubject()) + " "
                    + NodeFmtLib.strNT(t.getPredicate()) + " "
                    + NodeFmtLib.strNT(t.getObject()) + " .");
        }
    }

    private static final class ClosedWorldRuntimeException extends RuntimeException {
        ClosedWorldRuntimeException(String iri) { super(iri); }
    }
}
