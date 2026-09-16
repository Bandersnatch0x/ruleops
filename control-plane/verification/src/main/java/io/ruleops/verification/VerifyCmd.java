package io.ruleops.verification;

import com.fasterxml.jackson.databind.ObjectMapper;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Verify a compiled Candidate locally and write an EvidenceBundle skeleton.
 * Exit codes: 0 verification PASS, 1 verification FAIL, 2 usage error.
 *
 * <p>Usage: java -cp &lt;classpath&gt; io.ruleops.verification.VerifyCmd CANDIDATE_DIR OUT_DIR
 */
public final class VerifyCmd {

    public static void main(String[] args) throws Exception {
        if (args.length != 2) {
            System.err.println("usage: VerifyCmd CANDIDATE_DIR OUT_DIR");
            System.exit(2);
        }
        Path candidate = Path.of(args[0]);
        if (!Files.isDirectory(candidate)) {
            System.err.println("ERROR INPUT_NOT_FOUND: " + candidate);
            System.exit(2);
        }
        EvidenceBundle bundle;
        try {
            bundle = new LocalVerification().verify(candidate);
        } catch (Exception e) {
            if (System.getenv("VERIFY_DEBUG") != null) {
                e.printStackTrace();
            }
            System.err.println("ERROR VERIFICATION_ABORTED: " + e.getMessage());
            System.exit(1);
            return;
        }
        Path out = Path.of(args[1]);
        Files.createDirectories(out);
        Path evidence = out.resolve("evidence-bundle.json");
        Files.writeString(evidence, new ObjectMapper()
                .writerWithDefaultPrettyPrinter()
                .writeValueAsString(bundle.asJson()) + "\n");
        System.out.println("EVIDENCE " + evidence);
        System.out.println("VERDICT " + bundle.verdict() + " (" + EvidenceBundle.TRUST_LEVEL + ")");
        System.exit("PASS".equals(bundle.verdict()) ? 0 : 1);
    }

    private VerifyCmd() {}
}
