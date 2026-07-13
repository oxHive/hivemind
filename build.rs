fn main() {
    println!("cargo:rerun-if-changed=dashboard/dist");

    // include_dir! (src/http.rs) requires this path to exist at compile time.
    // dashboard/dist is gitignored build output, so a fresh clone won't have
    // it — create it empty so source builds compile without the dashboard.
    std::fs::create_dir_all("dashboard/dist").ok();

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
