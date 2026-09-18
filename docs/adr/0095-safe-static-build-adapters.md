# SAFE_STATIC build adapters use a closed metadata subset

The Maven and Gradle Workspace Resolver adapters operate in `SAFE_STATIC` mode.
They read only bounded project metadata and conventional Java source roots; no
build script, wrapper, dependency resolver, or project process is executed.

Maven module paths come only from literal `<module>` elements in POM metadata.
Gradle module paths come only from literal string arguments to `include`; dynamic
includes and custom `projectDir` expressions become `unresolved_facts`. Missing
classpath, build output, and other build-derived facts are never guessed. The
resulting module/file order is deterministic, and the descriptor digest covers
the canonical protobuf content with its digest field blank.
