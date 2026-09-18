---
status: accepted
---

# Use framed Protobuf over stdio for Backend Hosts

Rust Runtime and Backend Hosts communicate using length-delimited Protobuf frames over stdin/stdout, with stdout reserved for protocol traffic and stderr for structured logs. The interface includes protocol negotiation, request correlation, cancellation, deadlines and message-size limits, avoiding local network listeners and language-specific FFI while supporting Rust, Java, JavaScript and future hosts.
