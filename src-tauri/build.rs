use std::path::PathBuf;
use std::process::Command;

/// La cattura audio è scritta in Swift (ScreenCaptureKit e AVFoundation non
/// hanno un equivalente ragionevole in Rust) e linkata staticamente qui dentro.
/// Sta nello stesso processo di proposito: i permessi macOS sono legati al
/// processo che li richiede, e un binario separato firmato ad-hoc finirebbe per
/// chiederli per conto proprio.
fn build_swift_capture() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest.join("swift/BriefCapture.swift");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let library = out_dir.join("libbriefcapture.a");

    println!("cargo:rerun-if-changed={}", source.display());

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let optimization = if profile == "release" { "-O" } else { "-Onone" };

    let status = Command::new("swiftc")
        .args([
            "-emit-library",
            "-static",
            "-parse-as-library",
            "-module-name",
            "BriefCapture",
            "-target",
            "arm64-apple-macos13.0",
            optimization,
        ])
        .arg("-o")
        .arg(&library)
        .arg(&source)
        .status()
        .expect("swiftc non disponibile: installa gli Xcode Command Line Tools");

    assert!(status.success(), "compilazione di BriefCapture.swift fallita");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=briefcapture");

    // Il runtime Swift è già presente nel sistema da macOS 10.14.4: va linkato
    // dinamicamente, non ridistribuito.
    println!("cargo:rustc-link-search=native=/usr/lib/swift");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    for framework in [
        "ScreenCaptureKit",
        "AVFoundation",
        "CoreMedia",
        "CoreAudio",
        "AudioToolbox",
        "Foundation",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}

fn main() {
    build_swift_capture();
    tauri_build::build()
}
