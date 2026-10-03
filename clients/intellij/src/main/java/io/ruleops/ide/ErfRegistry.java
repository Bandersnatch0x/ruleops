package io.ruleops.ide;

import io.ruleops.ide.core.ThinClientCore;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

/** Tool-window state binding: release records are scoped to the workspace and explicit connection, never populated by receipts. */
final class ErfRegistry {
    record Request(String action, String resource, String cursor, ThinClientCore.RegistryPin pin, List<String> args) {}
    private String workspace = "", server = "", profile = "";
    private int timeoutMs;
    private String resource = "rules", source = "", descriptor = "";
    private List<ThinClientCore.RegistryPin> pins = List.of();
    private ThinClientCore.RegistryPin selected;
    private ThinClientCore.RegistryPage page;

    void configure(String workspace, String server, String profile, int timeoutMs) {
        if (!this.workspace.equals(workspace) || !this.server.equals(server) || !this.profile.equals(profile) || this.timeoutMs != timeoutMs) {
            pins = List.of(); source = ""; selected = null; descriptor = ""; page = null;
        }
        this.workspace = workspace; this.server = server; this.profile = profile; this.timeoutMs = timeoutMs;
    }
    void resource(String resource) {
        if (!List.of("rules", "standards").contains(resource)) throw new IllegalArgumentException("REGISTRY_TYPE_INVALID");
        if (!this.resource.equals(resource)) { this.resource = resource; page = null; selected = null; descriptor = ""; }
    }
    void loadPins(Map<String, Object> document, Path source) {
        var checked = ThinClientCore.projectRegistryPins(document);
        Path base = source.toAbsolutePath().getParent();
        pins = checked.stream().map(pin -> new ThinClientCore.RegistryPin(pin.resource(), pin.id(), pin.version(), pin.expectedDigest(), pin.provenance(),
            pin.deliveryDescriptor().isEmpty() ? "" : base.resolve(pin.deliveryDescriptor()).normalize().toString())).toList();
        this.source = source.toString(); selected = null; descriptor = "";
    }
    List<ThinClientCore.RegistryPin> pins() { return pins.stream().filter(pin -> pin.resource().equals(resource)).toList(); }
    void select(ThinClientCore.RegistryPin pin) {
        if (pin != null && !pins().contains(pin)) throw new IllegalArgumentException("INDEPENDENT_PIN_REQUIRED");
        selected = pin; descriptor = pin == null ? "" : pin.deliveryDescriptor();
    }
    void descriptor(String path) { descriptor = path; }
    String descriptor() { return descriptor; }
    String source() { return source; }
    void resetPage() { page = null; }
    boolean hasMore() { return page != null && page.hasMore(); }
    Request prepare(String action, boolean next, int limit) {
        String endpoint = profile.isBlank() ? server : ""; // Explicit profile takes precedence, never transport fallback.
        String cursor = next && hasMore() ? page.nextCursor() : null;
        if (next && (!"list".equals(action) || !hasMore())) throw new IllegalArgumentException("REGISTRY_NO_NEXT_PAGE");
        if (!"list".equals(action) && (selected == null || !pins().contains(selected))) throw new IllegalArgumentException("INDEPENDENT_PIN_REQUIRED");
        List<String> args = switch (action) {
            case "list" -> ThinClientCore.buildRegistryListArgs(resource, endpoint, profile, timeoutMs, limit, cursor);
            case "show" -> ThinClientCore.buildRegistryShowArgs(selected, endpoint, profile, timeoutMs);
            case "sync" -> ThinClientCore.buildRegistrySyncArgs(selected, descriptor, workspace, endpoint, profile, timeoutMs);
            default -> throw new IllegalArgumentException("REGISTRY_ACTION_INVALID");
        };
        return new Request(action, resource, cursor == null ? "" : cursor, selected, args);
    }
    String accept(Request request, Map<String, Object> receipt) {
        return switch (request.action()) {
            case "list" -> {
                page = ThinClientCore.projectRegistryPage(receipt, request.resource(), request.cursor());
                yield receipt.get("digestVerification") + " · authority=UNKNOWN · 未安装 / 不代表可执行\n" + page.items();
            }
            case "show" -> ThinClientCore.renderRegistryShow(receipt, request.pin());
            case "sync" -> ThinClientCore.renderRegistrySync(receipt, request.pin());
            default -> throw new IllegalArgumentException("REGISTRY_ACTION_INVALID");
        };
    }
}
