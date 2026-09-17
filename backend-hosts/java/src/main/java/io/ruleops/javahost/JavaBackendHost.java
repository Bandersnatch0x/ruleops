package io.ruleops.javahost;

import erf.backend.v0.AnalysisUnit;
import erf.backend.v0.BackendObservation;
import erf.backend.v0.CancelAck;
import erf.backend.v0.CapabilityDescriptor;
import erf.backend.v0.Completion;
import erf.backend.v0.DescribeResponse;
import erf.backend.v0.ExecuteComplete;
import erf.backend.v0.ExecuteEvent;
import erf.backend.v0.ExecuteRequest;
import erf.backend.v0.Hello;
import erf.backend.v0.HelloAck;
import erf.backend.v0.HostEnvelope;
import erf.backend.v0.PlannedUnit;
import erf.backend.v0.PlanRequest;
import erf.backend.v0.PlanResponse;
import erf.backend.v0.ProtocolError;
import erf.backend.v0.UnitFailureReason;
import erf.backend.v0.UnitOutcome;
import erf.backend.v0.UnitStatus;
import java.io.IOException;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.LinkedBlockingQueue;

/**
 * Session state machine of the Java Backend Host: framed stdio, a reader
 * thread feeding an inbox queue, and dispatch mirroring the Rust reference
 * fake host. Cooperative cancellation is detected by polling the inbox
 * between units during execute; the interrupted and remaining units are still
 * accounted with UnitStatus completeness facts.
 *
 * <p>Trusted configuration comes from the runtime through the environment:
 * ERF_REPO_ROOT (the mounted RepositorySnapshot root) and ERF_RULESET (the
 * compiled rule artifact). Signed RuleBundle distribution arrives with the
 * scan orchestration slice.
 */
final class JavaBackendHost {

    private final Framing framing;
    private final LinkedBlockingQueue<HostEnvelope> inbox =
            new LinkedBlockingQueue<>();
    private final int supportedMajor;
    private final PmdBackendAdapter adapter;
    private final Map<String, AnalysisUnit> plannedUnits =
            new HashMap<>();
    private final long delayMs;
    private long unitsReportedTotal;

    JavaBackendHost(Framing framing, int supportedMajor, PmdBackendAdapter adapter, long delayMs) {
        this.framing = framing;
        this.supportedMajor = supportedMajor;
        this.adapter = adapter;
        this.delayMs = delayMs;
    }

    /** Reader loop: runs on its own thread until stdin hits EOF. */
    void readLoop() {
        while (true) {
            byte[] frame;
            try {
                frame = framing.readFrame();
            } catch (IOException e) {
                return;
            }
            if (frame == null) {
                // stdin half-closed by the runtime: wake the dispatcher so it
                // can send the terminal Completion and exit.
                inbox.add(HostEnvelope.getDefaultInstance());
                return;
            }
            try {
                inbox.add(HostEnvelope.parseFrom(frame));
            } catch (IOException e) {
                // Fatal contract violation: report the structured error and
                // terminate the process (the dispatcher must not wait).
                fail(ProtocolError.Code.MALFORMED_FRAME, e.getMessage());
                Runtime.getRuntime().exit(1);
            }
        }
    }

    /** Dispatch loop on the main thread; blocks until the session ends. */
    void dispatchLoop() throws InterruptedException {
        while (true) {
            HostEnvelope envelope = inbox.take();
            if (envelope.getPayloadCase()
                    == HostEnvelope.PayloadCase.PAYLOAD_NOT_SET) {
                // EOF sentinel: normal session end after the half-close.
                complete(Completion.Status.COMPLETED, "session end", unitsReportedTotal);
                return;
            }
            if (!dispatch(envelope)) {
                return;
            }
        }
    }

    /** Returns false when the session must terminate. */
    private boolean dispatch(HostEnvelope envelope) {
        long correlation = envelope.getCorrelationId();
        switch (envelope.getPayloadCase()) {
            case HELLO -> hello(correlation, envelope.getHello());
            case DESCRIBE -> describe(correlation);
            case PLAN -> plan(correlation, envelope.getPlan());
            case EXECUTE -> execute(correlation, envelope.getExecute());
            case CANCEL -> {
                System.err.println("[host] cancel request received outside window");
                sendCancelAck(correlation, true);
            }
            case ERROR -> {
                System.err.println("{\"level\":\"error\",\"code\":"
                        + envelope.getError().getCode() + "}");
                return false;
            }
            default -> fail(ProtocolError.Code.UNCORRELATABLE,
                    "unexpected request frame: " + envelope.getPayloadCase());
        }
        return true;
    }

    private void hello(long correlation, Hello hello) {
        if (hello.getProtocolMajor() != supportedMajor) {
            error(correlation, ProtocolError.Code.PROTOCOL_MAJOR_MISMATCH,
                    "runtime speaks major " + hello.getProtocolMajor()
                            + ", host supports major " + supportedMajor);
            return;
        }
        send(HostEnvelope.newBuilder()
                .setCorrelationId(correlation)
                .setHelloAck(HelloAck.newBuilder()
                        .setProtocolMajor(hello.getProtocolMajor())
                        .setProtocolMinor(1)
                        .setHostName("erf-java-host")
                        .setHostVersion("0.1.0")
                        .setCapabilities(capabilities())
                        .setAcceptedLimits(hello.getLimits()))
                .build());
    }

    private void describe(long correlation) {
        send(HostEnvelope.newBuilder()
                .setCorrelationId(correlation)
                .setDescribeResponse(DescribeResponse.newBuilder()
                        .setCapabilities(capabilities()))
                .build());
    }

    private CapabilityDescriptor capabilities() {
        return CapabilityDescriptor.newBuilder()
                .addLanguages("java")
                .putFacts("java.ast", true)
                .putFacts("java.symbol-resolution", true)
                .addRuleKinds("SymbolRule")
                .setBackendId("pmd-java")
                .setBackendVersion(adapter.engineVersion())
                .build();
    }

    private void plan(long correlation, PlanRequest request) {
        var response = PlanResponse.newBuilder();
        for (AnalysisUnit unit : request.getUnitsList()) {
            plannedUnits.put(unit.getUnitId(), unit);
            // Deterministic refinement: one batch per unit set; the frozen
            // ScanPlan denominator is untouched (ADR-0036).
            response.addPlanned(PlannedUnit.newBuilder()
                    .setUnitId(unit.getUnitId())
                    .setBatchId("batch-0"));
        }
        send(HostEnvelope.newBuilder()
                .setCorrelationId(correlation)
                .setPlanResponse(response)
                .build());
    }

    private void execute(long correlation, ExecuteRequest request) {
        long reported = 0;
        for (int i = 0; i < request.getUnitIdsList().size(); i++) {
            String unitId = request.getUnitIdsList().get(i);
            AnalysisUnit unit = plannedUnits.get(unitId);
            if (unit == null) {
                reported += unitStatus(correlation, unitId,
                        UnitOutcome.FAILED,
                        UnitFailureReason.UNIT_INTERNAL_ERROR,
                        "unit was never planned");
                continue;
            }
            try {
                for (BackendObservation observation :
                        adapter.observe(unit)) {
                    sendEvent(correlation, ExecuteEvent.newBuilder()
                            .setObservation(observation)
                            .build());
                }
                reported += unitStatus(correlation, unitId,
                        UnitOutcome.EXECUTED,
                        UnitFailureReason.UNIT_FAILURE_REASON_UNSPECIFIED,
                        "");
            } catch (IOException e) {
                reported += unitStatus(correlation, unitId,
                        UnitOutcome.FAILED,
                        UnitFailureReason.UNIT_INTERNAL_ERROR,
                        String.valueOf(e.getMessage()));
            }
            // Cooperative cancellation point between units.
            if (i < request.getUnitIdsList().size() - 1 && drainCancel()) {
                for (String remaining : request.getUnitIdsList().subList(i + 1,
                        request.getUnitIdsList().size())) {
                    reported += unitStatus(correlation, remaining,
                            UnitOutcome.FAILED,
                            UnitFailureReason.CANCELLED_BY_RUNTIME,
                            "cancelled before completion");
                }
                complete(Completion.Status.CANCELLED,
                        "cancelled by runtime", reported);
                return;
            }
        }
        send(HostEnvelope.newBuilder()
                .setCorrelationId(correlation)
                .setExecuteComplete(ExecuteComplete.newBuilder()
                        .setUnitsReported(reported))
                .build());
    }

    /** Non-blocking check of the inbox for a pending CancelRequest. */
    private boolean drainCancel() {
        HostEnvelope envelope = inbox.poll();
        if (envelope == null) {
            return false;
        }
        if (envelope.getPayloadCase()
                == HostEnvelope.PayloadCase.CANCEL) {
            sendCancelAck(envelope.getCorrelationId(), true);
            return true;
        }
        fail(ProtocolError.Code.UNCORRELATABLE,
                "unexpected request during execute: " + envelope.getPayloadCase());
        return false;
    }

    private long unitStatus(
            long correlation,
            String unitId,
            UnitOutcome outcome,
            UnitFailureReason reason,
            String detail) {
        unitsReportedTotal++;
        sendEvent(correlation, ExecuteEvent.newBuilder()
                .setUnitStatus(UnitStatus.newBuilder()
                        .setUnitId(unitId)
                        .setOutcome(outcome)
                        .setReasonCode(reason)
                        .setDetail(detail == null ? "" : detail))
                .build());
        return 1;
    }

    private void sendCancelAck(long correlation, boolean accepted) {
        send(HostEnvelope.newBuilder()
                .setCorrelationId(correlation)
                .setCancelAck(CancelAck.newBuilder().setAccepted(accepted))
                .build());
    }

    private void sendEvent(long correlation, ExecuteEvent event) {
        send(HostEnvelope.newBuilder()
                .setCorrelationId(correlation)
                .setExecuteEvent(event)
                .build());
    }

    private void complete(
            Completion.Status status, String reason, long unitsReported) {
        send(HostEnvelope.newBuilder()
                .setCorrelationId(0)
                .setCompletion(Completion.newBuilder()
                        .setStatus(status)
                        .setReason(reason)
                        .setUnitsReported(unitsReported))
                .build());
    }

    private void error(long correlation, ProtocolError.Code code,
                       String detail) {
        send(HostEnvelope.newBuilder()
                .setCorrelationId(correlation)
                .setError(ProtocolError.newBuilder()
                        .setCode(code)
                        .setDetail(detail == null ? "" : detail))
                .build());
    }

    private void fail(ProtocolError.Code code, String detail) {
        error(0, code, detail);
    }

    private void send(HostEnvelope envelope) {
        try {
            framing.writeFrame(envelope.toByteArray());
        } catch (IOException e) {
            System.err.println("{\"level\":\"error\",\"msg\":\"write failed\"}");
        }
    }
}
