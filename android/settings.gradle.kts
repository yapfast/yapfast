import java.io.File

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

fun findRustlsMavenRepo(): File? {
    val explicit = providers.gradleProperty("rustlsMavenDir").orNull
        ?: System.getenv("RUSTLS_PLATFORM_VERIFIER_MAVEN")
    if (!explicit.isNullOrBlank()) {
        val dir = File(explicit)
        if (dir.isDirectory) return dir
    }

    val cargoHome = System.getenv("CARGO_HOME")?.let(::File)
        ?: File(System.getProperty("user.home"), ".cargo")
    val registrySrc = File(cargoHome, "registry/src")
    if (registrySrc.isDirectory) {
        val found = registrySrc.listFiles()
            ?.asSequence()
            ?.flatMap { it.listFiles()?.asSequence() ?: emptySequence() }
            ?.filter { it.isDirectory && it.name.startsWith("rustls-platform-verifier-android-") }
            ?.map { File(it, "maven") }
            ?.filter { it.isDirectory }
            ?.sortedByDescending { it.parentFile?.name }
            ?.firstOrNull()
        if (found != null) return found
    }
    return null
}

val rustlsMaven = findRustlsMavenRepo()

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        if (rustlsMaven != null) {
            maven {
                url = rustlsMaven.toURI()
            }
        }
    }
}

rootProject.name = "Yapfast"
include(":app")
