// ERF Thin Client（IntelliJ 平台）插件构建。离线交付：SDK/依赖需人工在离线镜像下解析。
// 纯逻辑核 io.ruleops.ide.core 无 IDE/网络依赖，可用 plain javac/java 离线单测
// （见 selftest/ 与 scripts/ide-client-check.sh）；本文件用于人工完成完整插件构建。

plugins {
    id("java")
    // 离线构建时由人工提供匹配版本；未联网时不解析。
    id("org.jetbrains.intellij") version "1.17.4"
}

group = "io.ruleops"
version = "0.1.0"

repositories {
    mavenCentral()
}

// IntelliJ 平台目标版本（人工按离线镜像可用版本调整）。
intellij {
    version.set("2024.1")
    type.set("IC")
    plugins.set(emptyList())
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

// IntelliJ 2024.1 runs on Java 17, even when Gradle itself uses JDK 21.
tasks.withType<JavaCompile>().configureEach { options.release.set(17) }

tasks {
    test {
        // Preserve the production Task.Backgroundable lifecycle in the headless SDK host.
        systemProperty("intellij.progress.task.ignoreHeadless", "true")
    }
    patchPluginXml {
        sinceBuild.set("241")
        untilBuild.set("")
    }
    // 纯核离线自测：不依赖 IntelliJ SDK，直接编译并运行 CoreSelfTest。
    register<JavaExec>("coreSelfTest") {
        val outDir = layout.buildDirectory.dir("core-selftest").get().asFile
        doFirst { outDir.mkdirs() }
        val core = file("src/main/java/io/ruleops/ide/core/ThinClientCore.java")
        val test = file("selftest/io/ruleops/ide/core/CoreSelfTest.java")
        // 由 scripts/ide-client-check.sh 以 plain javac/java 直接驱动；此处仅登记入口。
        mainClass.set("io.ruleops.ide.core.CoreSelfTest")
        classpath = files(outDir)
        dependsOn.clear()
    }
}

// One authoritative UI map; bundle it instead of maintaining a second command catalog.
tasks.processResources {
    from("../ide-command-map.json")
}
