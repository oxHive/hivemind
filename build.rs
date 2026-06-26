fn main() {
    // include_dir! panics at compile time if the directory doesn't exist.
    // Create an empty placeholder so CI builds succeed without the dashboard build step.
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dashboard/dist");
    if !dist.exists() {
        std::fs::create_dir_all(&dist).expect("failed to create dashboard/dist placeholder");
    }
    let index = dist.join("index.html");
    if !index.exists() {
        std::fs::write(&index, "<html><body>hivemind dashboard</body></html>")
            .expect("failed to create dashboard/dist/index.html placeholder");
    }
    println!("cargo:rerun-if-changed=dashboard/dist");

    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let is_tagged = std::process::Command::new("git")
        .args(["describe", "--exact-match", "--tags", "HEAD"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    println!("cargo:rustc-env=HIVEMIND_GIT_SHA={sha}");
    println!("cargo:rustc-env=HIVEMIND_IS_TAGGED={is_tagged}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");
}
