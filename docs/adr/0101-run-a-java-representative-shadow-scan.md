---
status: accepted
---

# Run a Java representative repository shadow scan

Phase 2 exercises one frozen, local Maven repository fixture through the
complete SAFE_STATIC resolver -> Rust Runner -> Java PMD Backend Host path.
The fixture contains one intentional `System.exit` violation so the path can
prove typed SourceSpan emission, unit accounting and local Gate aggregation;
the scan is repeated and both plan and SemanticResultDigest identities must
match.

The same harness also scans an `ExistingRule` Candidate,
`corp.java.avoid-duplicate-literals`, and asserts that the identity derived
from its validated manifest is retained in the ScanPlan and CLI output rather
than falling back to the first SymbolRule identity.

The fixture is a seam probe, not a claim about an external production
repository. The result remains `LOCAL_UNATTESTED`/`SHADOW`; this scan does not
establish signing, SCM publication, protected-branch authority or broad
repository coverage.
