import java.io.File

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

val cargoProfile = providers.gradleProperty("yapfastCargoProfile").orElse("fast").get()
val rustTarget = providers.gradleProperty("yapfastRustTarget").orElse("aarch64-linux-android").get()
val abiName = providers.gradleProperty("yapfastAbi").orElse("arm64-v8a").get()
val targetRoot = providers.gradleProperty("yapfastTargetDir")
    .map(::File)
    .orElse(File(rootDir.parentFile, "target"))
    .get()
val rustOutDir = File(targetRoot, "$rustTarget/$cargoProfile")
val nativeLib = File(rustOutDir, "libyapfast.so")
val webrtcJar = File(rustOutDir, "libwebrtc.jar")
val stagedJniDir = layout.buildDirectory.dir("generated/jniLibs")

val stageRustNativeLibs by tasks.registering(Sync::class) {
    from(nativeLib)
    into(stagedJniDir.map { it.dir(abiName) })
    doFirst {
        require(nativeLib.isFile) {
            "Missing native library: ${nativeLib.absolutePath}. Build yapfast-android first (e.g. via scripts/build-android)."
        }
        require(webrtcJar.isFile) {
            "Missing WebRTC Java archive: ${webrtcJar.absolutePath}. Build yapfast-android first."
        }
    }
}

android {
    namespace = "io.github.yapfast"
    compileSdk = 35
    val configuredNdk = System.getenv("ANDROID_NDK_HOME")
        ?: System.getenv("ANDROID_NDK_ROOT")
    if (!configuredNdk.isNullOrBlank()) {
        val ndkDir = File(configuredNdk)
        val propsFile = File(ndkDir, "source.properties")
        if (ndkDir.isDirectory && propsFile.isFile) {
            ndkPath = ndkDir.absolutePath
            propsFile.readLines()
                .firstOrNull { it.substringBefore('=').trim() == "Pkg.Revision" }
                ?.substringAfter('=', "")
                ?.trim()
                ?.takeIf { it.isNotEmpty() }
                ?.let { ndkVersion = it }
        }
    }

    defaultConfig {
        applicationId = "io.github.yapfast"
        minSdk = 28
        targetSdk = 35
        versionCode = 1
        versionName = "0.4.1"
        ndk {
            abiFilters += abiName
        }
    }

    sourceSets["main"].jniLibs.srcDir(stagedJniDir)

    val keystorePath = System.getenv("ANDROID_KEYSTORE_PATH")
    if (!keystorePath.isNullOrBlank()) {
        signingConfigs {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD") ?: ""
                keyAlias = System.getenv("ANDROID_KEY_ALIAS") ?: ""
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD") ?: ""
            }
        }
    }

    buildTypes {
        debug {
            isMinifyEnabled = false
        }
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            signingConfig = signingConfigs.findByName("release")
                ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    packaging {
        jniLibs {
            useLegacyPackaging = false
        }
    }
}

tasks.named("preBuild") {
    dependsOn(stageRustNativeLibs)
}

dependencies {
    implementation(files(webrtcJar))
    implementation("rustls:rustls-platform-verifier:0.1.1")
}
