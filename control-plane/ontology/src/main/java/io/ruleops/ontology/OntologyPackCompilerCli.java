package io.ruleops.ontology;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/** Small deterministic CLI used by the Phase 1 evidence workflow. */
public final class OntologyPackCompilerCli {
    private OntologyPackCompilerCli() {}

    public static void main(String[] args) throws Exception {
        if (args.length != 4) {
            throw new IllegalArgumentException("usage: <core.ttl> <extension.ttl> <revision> <out-dir>");
        }
        Path out = Path.of(args[3]);
        Files.createDirectories(out);
        OntologyContractPack pack = new OntologyContractPackCompiler()
                .compile(Path.of(args[0]), List.of(Path.of(args[1])), args[2]);
        Files.write(out.resolve("ontology-pack.nt"), pack.canonicalTriples());
        String manifest = "{\n"
                + "  \"identity\": \"" + pack.identity() + "\",\n"
                + "  \"revision\": \"" + pack.revision() + "\",\n"
                + "  \"digest\": \"" + pack.digest() + "\"\n"
                + "}\n";
        Files.writeString(out.resolve("manifest.json"), manifest, StandardCharsets.UTF_8);
        System.out.println("ONTOLOGY_PACK " + pack.identity());
    }
}
