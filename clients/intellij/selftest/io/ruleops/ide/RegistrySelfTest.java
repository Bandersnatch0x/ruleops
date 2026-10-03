package io.ruleops.ide;

import io.ruleops.ide.core.ThinClientCore;
import java.nio.file.Path;
import java.util.*;

/** Runs the exact non-SDK state binding used by the tool window, plus pure registry projections. */
public final class RegistrySelfTest {
    private static int passed;
    private static final String DIGEST = "sha256:" + "a".repeat(64);
    private static final String OTHER = "sha256:" + "b".repeat(64);
    private static final String PROFILE = "/approved/profile file.json";
    private static final String CURSOR = "opaque\0identity\0version \"\\中文";
    private static final Map<String, Object> PIN = Map.of("resource", "rules", "id", "rule with spaces", "version", "v 1", "expectedDigest", DIGEST,
        "provenance", "release job 42", "deliveryDescriptor", "signed delivery.json");
    private static Map<String, Object> changed(Map<String, Object> original, String key, Object value) {
        var copy = new LinkedHashMap<>(original); copy.put(key, value); return copy;
    }
    private static Map<String, Object> envelope(Map<String, Object> data, String verification) {
        var registry = new LinkedHashMap<String, Object>(Map.of("apiVersion", "rulefactory.io/v1", "schemaVersion", "asset-registry-v1", "authority", "UNKNOWN", "requestId", "registry-read"));
        registry.putAll(data);
        return Map.of("status", "RECEIVED_ONLY", "authority", "UNKNOWN", "digestVerification", verification, "registry", registry);
    }
    private static Map<String, Object> item() { return Map.of("type", "rules", "id", PIN.get("id"), "version", PIN.get("version"), "state", "PUBLISHED", "digest", DIGEST); }
    private static Map<String, Object> page(String cursor, String next, boolean more) { return envelope(Map.of("cursor", cursor, "nextCursor", next, "hasMore", more, "items", List.of(item())), "FORMAT_ONLY"); }
    private static Map<String, Object> show(Map<String, Object> item) { return envelope(Map.of("resource", item), "PIN_MATCH"); }
    private static Map<String, Object> sync() {
        var result = new LinkedHashMap<String, Object>();
        result.put("status", "CACHED"); result.put("authority", "UNKNOWN"); result.put("digestVerification", "PIN_MATCH");
        result.put("artifactKind", "SignedRuleBundle"); result.put("bundleDigest", OTHER); result.put("destination", "/actual/cache/bundle.zip");
        result.put("overwritePolicy", "IMMUTABLE_DIGEST_ADDRESSED"); result.put("offlineAvailable", true);
        result.put("provenance", Map.of("descriptor", "/release/delivery.json", "trustRoot", "RULEOPS_TRUST_ROOT")); result.put("registry", show(item()).get("registry"));
        return result;
    }
    private static void check(boolean value, String name) { if (!value) throw new AssertionError(name); passed++; }
    private static void rejects(Runnable work, String code) {
        try { work.run(); } catch (IllegalArgumentException error) { check(error.getMessage().contains(code), error.toString()); return; }
        throw new AssertionError("expected " + code);
    }
    private static String arg(List<String> args, String flag) { return args.get(args.indexOf(flag) + 1); }
    public static void main(String[] args) {
        var pins = ThinClientCore.projectRegistryPins(Map.of("pins", List.of(PIN, changed(changed(PIN, "version", "v 2"), "expectedDigest", OTHER))));
        var pin = pins.getFirst();
        var list = ThinClientCore.buildRegistryListArgs("rules", "", PROFILE, 5000, 50, CURSOR);
        check(arg(list, "--cursor-json").equals("\"opaque\\u0000identity\\u0000version \\\"\\\\中文\""), "opaque NUL JSON argv");
        check(list.stream().noneMatch(value -> value.indexOf(0) >= 0), "no raw NUL process args");
        check(arg(list, "--connection-profile").equals(PROFILE), "profile is single FILE arg");
        rejects(() -> ThinClientCore.buildRegistryListArgs("rules", "http://127.0.0.1/v1", PROFILE, 5, 10, null), "EXACTLY_ONE");
        rejects(() -> ThinClientCore.buildRegistryListArgs("rules", "", "", 5, 10, null), "EXACTLY_ONE");
        rejects(() -> ThinClientCore.buildRegistryListArgs("rules", "https://user:secret@example.com/v1", "", 5, 10, null), "ENDPOINT");
        rejects(() -> ThinClientCore.buildRegistryListArgs("rules", "", PROFILE, 5, 257, null), "LIMIT");
        var showArgs = ThinClientCore.buildRegistryShowArgs(pins.get(1), "", PROFILE, 5000);
        check(showArgs.get(3).equals("rule with spaces") && arg(showArgs, "--version").equals("v 2") && arg(showArgs, "--expected-digest").equals(OTHER), "complete second identity, not first digest");
        rejects(() -> ThinClientCore.projectRegistryPins(Map.of("pins", List.of(changed(PIN, "provenance", "")))), "provenance");
        var syncArgs = ThinClientCore.buildRegistrySyncArgs(pin, "/external signed.json", "/workspace with spaces", "", PROFILE, 5000);
        check(syncArgs.subList(0, 4).equals(List.of("registry", "sync", "rules", pin.id())), "sync command");
        check(arg(syncArgs, "--delivery-descriptor").equals("/external signed.json") && arg(syncArgs, "--repo-root").equals("/workspace with spaces"), "independent descriptor and root argv");
        String verified = ThinClientCore.renderRegistryShow(show(item()), pin);
        check(verified.contains("PIN_MATCH") && verified.contains("UNKNOWN") && verified.contains("未安装"), "pin comparison is not installation");
        check(ThinClientCore.renderRegistryShow(changed(show(item()), "digestVerification", "UNKNOWN"), pin).contains("UNKNOWN"), "unknown remains unknown");
        rejects(() -> ThinClientCore.renderRegistryShow(show(changed(item(), "digest", OTHER)), pin), "DIGEST_MISMATCH");
        rejects(() -> ThinClientCore.renderRegistryShow(show(changed(item(), "version", "v 2")), pin), "IDENTITY_MISMATCH");
        rejects(() -> ThinClientCore.projectRegistryPage(page("stale", CURSOR, true), "rules", ""), "PAGE_INVALID");
        rejects(() -> ThinClientCore.projectRegistryPage(page("", CURSOR, true), "standards", ""), "RESPONSE_INVALID");
        check(ThinClientCore.projectRegistryPage(page("", CURSOR, true), "rules", "").nextCursor().equals(CURSOR), "page cursor untouched");
        String cached = ThinClientCore.renderRegistrySync(sync(), pin);
        check(cached.contains(OTHER) && cached.contains("/actual/cache/bundle.zip") && cached.contains("IMMUTABLE_DIGEST_ADDRESSED") && cached.contains("offlineAvailable：true"), "render receipt facts, not store digest");
        check(ThinClientCore.renderRegistrySync(changed(sync(), "offlineAvailable", false), pin).contains("offlineAvailable：false"), "no offline inference");
        rejects(() -> ThinClientCore.renderRegistrySync(changed(sync(), "bundleDigest", ""), pin), "FIELD");
        check(ThinClientCore.renderRegistrySync(Map.of("status", "UNKNOWN", "authority", "UNKNOWN"), pin).contains("离线可用性未知"), "unknown not cached");

        var binding = new ErfRegistry();
        binding.configure("/workspace with spaces", "http://127.0.0.1:9/v1", PROFILE, 5000);
        var first = binding.prepare("list", false, 50);
        check(first.args().contains("--connection-profile") && !first.args().contains("--server"), "binding profile preference");
        binding.accept(first, page("", CURSOR, true));
        check(binding.pins().isEmpty(), "response never populates independent pins");
        rejects(() -> binding.prepare("show", false, 50), "PIN_REQUIRED");
        var next = binding.prepare("list", true, 50);
        check(arg(next.args(), "--cursor-json").equals(ThinClientCore.registryCursorJson(CURSOR)), "binding paging opaque argv");
        binding.accept(next, page(CURSOR, "end", false));
        rejects(() -> binding.prepare("list", true, 50), "NO_NEXT_PAGE");
        binding.loadPins(Map.of("pins", List.of(PIN, changed(changed(PIN, "version", "v 2"), "expectedDigest", OTHER))), Path.of("/release/pins.json"));
        rejects(() -> binding.prepare("show", false, 50), "PIN_REQUIRED");
        binding.select(binding.pins().get(1));
        var second = binding.prepare("show", false, 50);
        check(second.pin().version().equals("v 2") && second.pin().expectedDigest().equals(OTHER), "binding one whole release record");
        rejects(() -> binding.accept(second, show(item())), "IDENTITY_MISMATCH");
        check(binding.prepare("show", false, 50).pin().equals(second.pin()), "response mismatch cannot replace pin");
        var delivery = binding.prepare("sync", false, 50);
        check(arg(delivery.args(), "--delivery-descriptor").equals("/release/signed delivery.json"), "relative release descriptor path");
        binding.descriptor("/other/external.json");
        check(arg(binding.prepare("sync", false, 50).args(), "--delivery-descriptor").equals("/other/external.json"), "external descriptor override");
        check(binding.prepare("sync", false, 50).pin().equals(second.pin()), "descriptor never replaces pin");
        binding.resource("standards"); rejects(() -> binding.prepare("show", false, 50), "PIN_REQUIRED");
        binding.resource("rules"); binding.select(binding.pins().getFirst());
        binding.configure("/workspace with spaces", "http://127.0.0.1:9/v1", "/different/profile.json", 5000);
        check(binding.pins().isEmpty() && binding.descriptor().isEmpty() && !binding.hasMore(), "connection reset invalidates all state");
        binding.loadPins(Map.of("pins", List.of(PIN)), Path.of("/release/pins.json")); binding.select(binding.pins().getFirst());
        binding.configure("/other workspace", "http://127.0.0.1:9/v1", "/different/profile.json", 5000);
        check(binding.pins().isEmpty(), "workspace reset invalidates pins");
        System.out.println("intellij-registry-selftest: pass=" + passed + " fail=0 (core + actual tool-window state binding; NOT SDK host test)");
    }
}
