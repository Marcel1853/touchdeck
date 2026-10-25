import java.util.Properties

plugins {
    id("com.android.application")
}

// Eigener Signierschlüssel (liegt nur lokal in keystore/, nicht im Code).
val signing = Properties().apply {
    val file = rootProject.file("keystore/keystore.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}

android {
    namespace = "de.touchdeck.app"
    compileSdk = 37

    defaultConfig {
        applicationId = "de.touchdeck.app"
        minSdk = 26
        targetSdk = 37
        versionCode = 12
        versionName = "0.8.2"
    }

    signingConfigs {
        create("release") {
            if (signing.isNotEmpty()) {
                storeFile = rootProject.file("keystore/${signing["storeFile"]}")
                storePassword = signing["storePassword"] as String
                keyAlias = signing["keyAlias"] as String
                keyPassword = signing["keyPassword"] as String
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.getByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    // QR-Code-Scanner. Er braucht androidx.core (Kamera-Berechtigung), gibt das aber nicht selbst an.
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")
    implementation("androidx.core:core:1.19.0")
}
