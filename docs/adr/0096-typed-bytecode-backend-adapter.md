# Keep SpotBugs bytecode findings behind the shared Host contract

The Java Backend Host keeps PMD and SpotBugs behind one backend-neutral adapter
seam. PMD remains the source-oriented `SymbolRule` path; SpotBugs is selected
explicitly as `spotbugs-java`, advertises `BytecodeRule`, and consumes only
repository-relative compiled class files supplied by the Runtime. It never
invokes a project build or silently falls back between engines.

SpotBugs findings are converted to policy-neutral `BackendObservation` values
with a stable adapter rule identity and a typed `BytecodeMember` location. A
missing, malformed, duplicated, or symlink-crossing class input fails the unit
instead of producing a clean result.
