package io.ruleops.ide.actions;

// FindingView 载入：读取用户选定的 digest-only JSON，解析为 Map 后交 ThinClientCore
// 做闭世界/隐私/路径门禁。解析用平台自带 Gson，绝不反序列化为强类型以免夹带源码字段。

import com.google.gson.Gson;
import io.ruleops.ide.core.ThinClientCore;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public final class FindingViewLoader {
    private static final Gson GSON = new Gson();

    public static final class Loaded {
        public final Map<String, Object> view;
        public final ThinClientCore.Check check;
        Loaded(Map<String, Object> view, ThinClientCore.Check check) {
            this.view = view;
            this.check = check;
        }
    }

    @SuppressWarnings("unchecked")
    public static Loaded load(Path jsonPath) throws Exception {
        String text = Files.readString(jsonPath);
        Object parsed = GSON.fromJson(text, Object.class);
        Map<String, Object> view = normalize(parsed);
        ThinClientCore.Check check = ThinClientCore.validateFindingView(view);
        return new Loaded(view, check);
    }

    // 递归把 Gson 的 Double 数字规约为 Integer（当为整值时），使 line/column 校验成立。
    @SuppressWarnings("unchecked")
    private static Object normalizeValue(Object v) {
        if (v instanceof Map<?, ?> m) {
            return normalize(m);
        }
        if (v instanceof List<?> list) {
            java.util.ArrayList<Object> out = new java.util.ArrayList<>();
            for (Object item : list) out.add(normalizeValue(item));
            return out;
        }
        if (v instanceof Double d && d == Math.rint(d) && !d.isInfinite()) {
            return (int) (double) d;
        }
        return v;
    }

    private static Map<String, Object> normalize(Object parsed) {
        Map<String, Object> out = new LinkedHashMap<>();
        if (parsed instanceof Map<?, ?> m) {
            for (Map.Entry<?, ?> e : m.entrySet()) {
                out.put(String.valueOf(e.getKey()), normalizeValue(e.getValue()));
            }
        }
        return out;
    }
}
