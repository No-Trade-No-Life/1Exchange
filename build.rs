use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=BUILD_TIME");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-env-changed=GITHUB_RUN_ID");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");

    set_build_env("ONE_EXCHANGE_BUILD_TIME", "BUILD_TIME", &utc_now());
    set_build_env("ONE_EXCHANGE_GIT_REF", "GITHUB_REF_NAME", "unknown");
    set_build_env("ONE_EXCHANGE_GITHUB_RUN_ID", "GITHUB_RUN_ID", "unknown");
    set_build_env("ONE_EXCHANGE_GIT_SHA", "GITHUB_SHA", &git_sha());
}

fn set_build_env(key: &str, source: &str, fallback: &str) {
    let value = std::env::var(source).unwrap_or_else(|_| fallback.to_string());
    println!("cargo:rustc-env={key}={value}");
}

fn git_sha() -> String {
    command_output("git", &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string())
}

fn utc_now() -> String {
    command_output("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]).unwrap_or_else(|| "unknown".to_string())
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(stdout.trim().to_string())
}
