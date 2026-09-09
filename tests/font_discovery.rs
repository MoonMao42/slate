//! Real CLI against private, non-catalog filename fixtures. No real fonts,
//! package manager, font cache, rendering engine or network are invoked.
use slate_cli::{config::ConfigManager, env::SlateEnv};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::PathBuf,
    time::Duration,
};

const FONT: &[u8] = b"\0\x01\0\0private-font-discovery-fixture";
struct Fixture {
    _temp: tempfile::TempDir,
    env: SlateEnv,
    fonts: PathBuf,
    bin: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let fonts = slate_cli::platform::fonts::user_font_dir(&env);
        fs::create_dir_all(&fonts).unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        for name in ["fc-cache", "brew", "curl"] {
            let binary = bin.join(name);
            fs::write(
                &binary,
                "#!/bin/sh\nprintf 'unexpected' > \"$HOME/native-command-ran\"\nexit 90\n",
            )
            .unwrap();
            fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self {
            _temp: temp,
            env,
            fonts,
            bin,
        }
    }
    fn run(&self, name: &str) -> assert_cmd::assert::Assert {
        let assertion = assert_cmd::Command::cargo_bin("slate")
            .unwrap()
            .env_clear()
            .env("HOME", self.env.home())
            .env("SLATE_HOME", self.env.home())
            .env("PATH", &self.bin)
            .env("SHELL", "/bin/bash")
            .env("NO_COLOR", "1")
            .args(["font", name])
            .timeout(Duration::from_secs(10))
            .assert();
        assert!(!self.env.home().join("native-command-ran").exists());
        assertion
    }
}

#[test]
fn font_discovery_cli_selects_nested_uppercase_fonts_and_opentype_collections() {
    for (extension, bytes) in [
        ("TTF", FONT),
        ("oTc", b"ttcfprivate-collection-fixture".as_slice()),
    ] {
        let f = Fixture::new();
        let nested = f.fonts.join("family/subfamily");
        fs::create_dir_all(&nested).unwrap();
        let path = nested.join(format!("SlateDiscoveryFixtureNerdFont-Regular.{extension}"));
        fs::write(&path, bytes).unwrap();
        f.run("SlateDiscoveryFixture Nerd Font").success();
        assert_eq!(
            ConfigManager::with_env(&f.env)
                .unwrap()
                .get_current_font()
                .unwrap()
                .as_deref(),
            Some("SlateDiscoveryFixture Nerd Font")
        );
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn font_discovery_cli_does_not_select_a_directory_empty_file_or_text_impostor() {
    for kind in ["directory", "empty", "text"] {
        let f = Fixture::new();
        let path = f.fonts.join("SlateDiscoveryFixtureNerdFont-Regular.ttf");
        match kind {
            "directory" => fs::create_dir(&path).unwrap(),
            "empty" => fs::write(&path, b"").unwrap(),
            "text" => fs::write(&path, b"not a font despite the name").unwrap(),
            _ => unreachable!(),
        }
        f.run("SlateDiscoveryFixture Nerd Font").failure();
        assert!(!f.env.managed_file("current-font").exists());
        assert!(path.exists());
    }
}

#[test]
fn font_discovery_cli_preserves_partial_scan_evidence_and_usable_known_candidates() {
    let f = Fixture::new();
    fs::write(
        f.fonts.join("SlateDiscoveryFixtureNerdFont-Regular.ttf"),
        FONT,
    )
    .unwrap();
    symlink(f.fonts.join("missing"), f.fonts.join("broken.ttf")).unwrap();
    let output = f
        .run("SlateUnknownFixture Nerd Font")
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Font discovery is incomplete"), "{stderr}");
    assert!(!stderr.contains("Downloading") && !stderr.contains("not found"));
    assert!(!f.env.managed_file("current-font").exists());
    f.run("SlateDiscoveryFixture Nerd Font").success();
}
