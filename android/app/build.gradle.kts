plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

android {
    namespace = "io.github.danielpereod.opdshelf"
    compileSdk = 37

    defaultConfig {
        applicationId = "io.github.danielpereod.opdshelf"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    // One key shared by every build, here and in CI, so a newer APK installs over an older one
    // without uninstalling it. It is for test builds only: anyone with the repository can sign with it.
    signingConfigs {
        create("test") {
            storeFile = file("opdshelf-test.keystore")
            storePassword = "opdshelf"
            keyAlias = "opdshelf"
            keyPassword = "opdshelf"
        }
    }

    buildTypes {
        debug {
            signingConfig = signingConfigs.getByName("test")
        }
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("test")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
        // Readium uses java.time on older Android versions
        isCoreLibraryDesugaringEnabled = true
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    // The xpointer conversion is shared with the web reader
    sourceSets.getByName("main").assets.directories.add("build/generated/sharedAssets")

    testOptions {
        unitTests.isIncludeAndroidResources = true
    }

    packaging {
        resources.excludes += setOf("META-INF/LICENSE*", "META-INF/NOTICE*", "META-INF/*.kotlin_module")
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
        optIn.addAll("org.readium.r2.shared.ExperimentalReadiumApi", "org.readium.r2.shared.DelicateReadiumApi")
    }
}

val copySharedAssets = tasks.register<Copy>("copySharedAssets") {
    from(rootProject.file("../static/reader/xpointer.js"))
    into(layout.buildDirectory.dir("generated/sharedAssets/reader"))
}
tasks.named("preBuild") { dependsOn(copySharedAssets) }

dependencies {
    val readium = "3.4.0"
    implementation("org.readium.kotlin-toolkit:readium-shared:$readium")
    implementation("org.readium.kotlin-toolkit:readium-streamer:$readium")
    implementation("org.readium.kotlin-toolkit:readium-navigator:$readium")
    implementation("org.readium.kotlin-toolkit:readium-adapter-pdfium:$readium")

    implementation(platform("androidx.compose:compose-bom:2025.10.00"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.4")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.4")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.fragment:fragment-ktx:1.8.9")
    implementation("com.google.android.material:material:1.13.0")
    implementation("androidx.work:work-runtime-ktx:2.10.5")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("io.coil-kt:coil-compose:2.7.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")

    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.1.5")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.10.2")
    testImplementation("com.squareup.okhttp3:mockwebserver:4.12.0")
    // Screenshots of the screens, rendered on the JVM
    testImplementation(platform("androidx.compose:compose-bom:2025.10.00"))
    testImplementation("org.robolectric:robolectric:4.16")
    testImplementation("androidx.compose.ui:ui-test-junit4")
    testImplementation("androidx.test:core:1.7.0")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}

tasks.withType<Test>().configureEach {
    // Robolectric's Android runtime from a local folder instead of downloading it, when given
    System.getenv("ROBOLECTRIC_DEPS")?.let {
        systemProperty("robolectric.offline", "true")
        systemProperty("robolectric.dependency.dir", it)
    }
    // Screenshots run only when asked for: they need Robolectric's 200 MB Android runtime
    val screenshots = System.getenv("SCREENSHOT_DIR")
    if (screenshots == null) filter { excludeTestsMatching("*ScreenshotTest") }
    screenshots?.let { environment("SCREENSHOT_DIR", it) }
    System.getenv("SCREENSHOT_SERVER")?.let { environment("SCREENSHOT_SERVER", it) }
}
