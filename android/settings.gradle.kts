pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // Readium's PDF adapter depends on AndroidPdfViewer, published only there
        maven("https://jitpack.io") { content { includeGroup("com.github.marain87") } }
    }
}

rootProject.name = "OPDShelf"
include(":app")
