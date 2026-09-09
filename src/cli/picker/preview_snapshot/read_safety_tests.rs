use super::*;

#[test]
fn preview_read_safety_rejects_oversized_files_before_collecting_their_bytes() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("large-config");
    let file = fs::File::create(&path).unwrap();
    file.set_len(crate::config::file_read::MAX_TOOL_CONFIG_BYTES + 1)
        .unwrap();
    let error = match read_state(&path) {
        Err(error) => error,
        Ok(_) => panic!("oversized preview input was accepted"),
    };
    assert!(error.to_string().contains("8 MiB per file"));
    assert_eq!(
        file.metadata().unwrap().len(),
        crate::config::file_read::MAX_TOOL_CONFIG_BYTES + 1
    );
}

#[test]
fn preview_read_safety_keeps_binary_modes_and_resolved_dotfile_links() {
    use std::os::unix::fs::symlink;
    let home = tempfile::tempdir().unwrap();
    let real = home.path().join("dotfiles");
    fs::create_dir(&real).unwrap();
    let target = real.join("config");
    fs::write(&target, [0, 0xff, b'\n']).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o1640)).unwrap();
    let mode = fs::metadata(&target).unwrap().permissions().mode();
    let link = home.path().join("entry-link");
    symlink(&target, &link).unwrap();
    let expected = FileState::Present {
        bytes: vec![0, 0xff, b'\n'],
        mode,
    };
    assert!(read_state(&resolve_destination(&link).unwrap()).unwrap() == expected);
    assert!(
        read_state(&link).is_err(),
        "raw destination reads must not follow final links"
    );
    symlink(&real, home.path().join("alias")).unwrap();
    assert!(read_state(&home.path().join("alias/config")).unwrap() == expected);
    assert!(read_state(&home.path().join("missing/nested/config")).unwrap() == FileState::Absent);
    symlink(home.path().join("absent"), home.path().join("dangling")).unwrap();
    assert!(read_state(&home.path().join("dangling/config")).is_err());
    assert!(read_state(&target.join("child")).is_err());
    assert!(read_state(&real).is_err());

    let mut remaining = 3;
    assert!(read_state_with_budget(&target, &mut remaining).unwrap() == expected);
    assert_eq!(remaining, 0);
    assert!(read_state_with_budget(&target, &mut remaining).is_err());
    assert_eq!(remaining, 0);
}

#[test]
fn preview_read_safety_capture_enforces_the_aggregate_budget_without_publishing_a_record() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let paths = [
        "managed/ghostty/theme.conf",
        "managed/ghostty/font.conf",
        "managed/ghostty/opacity.conf",
    ];
    let sizes = [6 * 1024 * 1024, 6 * 1024 * 1024, 4 * 1024 * 1024 + 1];
    for (path, length) in paths.iter().zip(sizes) {
        let path = env.managed_file(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = fs::File::create(path).unwrap();
        file.set_len(length).unwrap();
    }
    let error = match PreviewSnapshot::capture(&env) {
        Err(error) => error,
        Ok(_) => panic!("oversized aggregate preview state was accepted"),
    };
    assert!(error.to_string().contains("16 MiB per captured state"));
    assert!(!crate::config::write_guard::record_path(&env).exists());
    for (path, length) in paths.iter().zip(sizes) {
        assert_eq!(fs::metadata(env.managed_file(path)).unwrap().len(), length);
    }
    let lock = crate::config::write_guard::open_lock(&env, false)
        .unwrap()
        .unwrap();
    assert!(crate::config::write_guard::try_lock(&lock).unwrap());
}
