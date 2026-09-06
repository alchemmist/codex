fn main() {
    let commit = git(&["rev-parse", "--short=10", "HEAD"]).unwrap_or_else(|| "dev".into());
    let mut paths = vec!["HEAD".into(), "packed-refs".into()];
    if let Some(reference) = git(&["symbolic-ref", "--quiet", "HEAD"]) {
        paths.push(reference);
    }
    for path in paths {
        if let Some(path) = git(&["rev-parse", "--git-path", &path]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    println!("cargo:rerun-if-changed=Cargo.toml");
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo supplies package version");
    let version = if version == "0.0.0" {
        format!("{version}+{commit}")
    } else {
        version
    };
    println!("cargo:rustc-env=ANTEX_BUILD_VERSION={version}");
}

fn git(args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_owned())
}
