plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// versionName / versionCode are injected by CI (-PversionName= -PversionCode=);
// local defaults keep IDE builds working.
val versionNameProp: String = (project.findProperty("versionName") as String?) ?: "0.2.0"
val versionCodeProp: Int = (project.findProperty("versionCode") as String?)?.toIntOrNull() ?: 1

android {
    namespace = "dev.sakura.llmgateway"
    compileSdk = 34

    defaultConfig {
        applicationId = "dev.sakura.llmgateway"
        minSdk = 26
        targetSdk = 34
        versionCode = versionCodeProp
        versionName = versionNameProp
    }

    // Universal APK: jniLibs for all three ABIs are packaged together.
    splits {
        abi {
            isEnable = false
        }
    }

    signingConfigs {
        create("release") {
            // CI injects these via -P android.keystore.* properties (from GitHub Secrets).
            val storeFileProp = project.findProperty("android.keystore.path") as String?
            if (storeFileProp != null) {
                storeFile = file(storeFileProp)
                storePassword = project.findProperty("android.keystore.password") as String?
                keyAlias = project.findProperty("android.keystore.alias") as String?
                keyPassword = project.findProperty("android.keystore.keypassword") as String?
            }
            isV1SigningEnabled = true
            isV2SigningEnabled = true
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            val configured = project.findProperty("android.keystore.path") != null
            signingConfig = if (configured) signingConfigs.getByName("release") else null
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        compose = true
    }
    packaging {
        jniLibs {
            // The gateway binary lives in jniLibs as libllmgateway.so per ABI;
            // keep it uncompressed extraction-free? No: we need a real file path
            // to exec, so allow extraction.
            useLegacyPackaging = true
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.4")
    implementation("androidx.activity:activity-compose:1.9.1")
    implementation(platform("androidx.compose:compose-bom:2024.08.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.webkit:webkit:1.11.0")
}
