use super::*;
use std::os::unix::fs::symlink;

#[test]
fn bash_startup_selection_preserves_login_precedence_and_linux_rc_convention() {
    for present in 0..16 {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        let paths = env.bash_startup_paths();
        for (index, path) in paths.iter().enumerate() {
            if present & (1 << index) != 0 {
                fs::write(path, "# user config\n").unwrap();
            }
        }
        let expected = paths[1..]
            .iter()
            .enumerate()
            .find(|(index, _)| present & (1 << (index + 1)) != 0)
            .map_or_else(|| env.bash_profile_path(), |(_, path)| path.clone());
        assert_eq!(
            env.bash_integration_path_for_login(true),
            expected,
            "{present}"
        );
        assert_eq!(
            env.bash_integration_path_for_login(false),
            env.bashrc_path()
        );
        assert_eq!(
            fs::read_dir(td.path()).unwrap().count(),
            (present as u32).count_ones() as usize
        );
    }
}

#[test]
fn bash_startup_selection_does_not_bypass_unsafe_higher_priority_entries() {
    for kind in ["dangling", "directory", "fifo"] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        fs::write(env.bashrc_path(), "# rc\n").unwrap();
        fs::write(env.shell_profile_path(), "# shared profile\n").unwrap();
        let candidate = env.bash_login_path();
        match kind {
            "dangling" => symlink(td.path().join("absent"), &candidate).unwrap(),
            "directory" => fs::create_dir(&candidate).unwrap(),
            "fifo" => {
                let path =
                    std::ffi::CString::new(candidate.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            _ => unreachable!(),
        }
        assert_eq!(env.bash_integration_path_for_login(true), candidate);
        assert_eq!(
            env.bash_integration_path_for_login(false),
            env.bashrc_path()
        );
    }
}
