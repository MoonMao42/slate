use super::*;

fn env() -> SlateEnv {
    SlateEnv::with_home("/private/clean test/用户".into())
}

fn output(input: &[u8], edit: Edit) -> Vec<u8> {
    match edit {
        Edit::Keep(None) => input.to_vec(),
        Edit::Replace(bytes) => bytes,
        _ => panic!("unexpected edit kind"),
    }
}

#[test]
fn clean_alacritty_leaves_unrelated_documents_byte_identical() {
    let env = env();
    let root = env.config_dir().join("managed/alacritty");
    for input in [
        "# user's empty section\r\n[general]\r\n".to_string(),
        "[general]\nimport = [] # intentionally empty\n".to_string(),
        "[general]\nimport = [\n  # user's import\n  'my theme.toml', # keep this\n  42,\n]\n".to_string(),
        format!("[general]\nimport = ['{}-custom/theme.toml', '/mirror{}/theme.toml', '{}/../user/theme.toml']\n", root.display(), root.display(), root.display()),
        "[general]\nimport = 'not an array'\n".to_string(),
    ] {
        assert!(
            matches!(alacritty(&env, input.as_bytes()).unwrap(), Edit::Keep(None)),
            "unrelated document must not be rewritten"
        );
    }
}

#[test]
fn clean_alacritty_removes_only_value_tokens_and_separators() {
    let env = env();
    let managed = env.config_dir().join("managed/alacritty/colors.toml");
    let input = format!(
        "# PRIVATE_CONTENT\r\n[general] # header\r\nimport = [\r\n  # keep explanation, including a comma ,\r\n  '{}', # keep this note too,\r\n  'user.toml', # retained import\r\n  42, # invalid application value is still user's data\r\n] # tail\r\n[window]\r\nopacity = 0.8\r\n",
        managed.display()
    );
    let expected = input.replace(&format!("'{}',", managed.display()), "");
    let actual = output(input.as_bytes(), alacritty(&env, input.as_bytes()).unwrap());
    assert_eq!(actual, expected.as_bytes());
    assert!(matches!(
        alacritty(&env, &actual).unwrap(),
        Edit::Keep(None)
    ));
}

#[test]
fn clean_kitty_keeps_neighboring_paths_directives_and_sockets() {
    let env = env();
    let root = env.config_dir().join("managed/kitty");
    let user = format!(
        "# PRIVATE_CONTENT\r\ninclude {root}-custom/theme.conf\r\ninclude /mirror{root}/theme.conf\r\ninclude {root}/../user/theme.conf\r\ninclude_extra {root}/theme.conf\r\nglobinclude {root}/*.conf\r\nlisten_on unix:/tmp/kitty-slate-personal\r\nlisten_on unix:/tmp/kitty-slate/other\r\nlisten_on tcp:kitty-slate:1234\r\nlisten_on_extra unix:/tmp/kitty-slate\r\n# include {root}/theme.conf\r\n",
        root = root.display()
    );
    let mut input = user.as_bytes().to_vec();
    input.extend_from_slice(&[0xff, b'\n']);
    let expected = input.clone();
    input.extend_from_slice(
        format!(
            " \tinclude\t{}/theme.conf\r\nlisten_on unix:/tmp/kitty-slate\r\n",
            root.display()
        )
        .as_bytes(),
    );
    assert_eq!(output(&input, kitty(&env, &input).unwrap()), expected);
    assert!(matches!(kitty(&env, &expected).unwrap(), Edit::Keep(None)));
}

#[test]
fn clean_kitty_handles_whole_continuations_without_expanding_user_input() {
    let env = env();
    let root = env.config_dir().join("managed/kitty");
    let user = format!(
        "# retained comment\r\n\\include {root}/theme.conf\r\ninclude {root}\r\n  \\-custom/theme.conf\r\ninclude {root}/$USER/theme.conf\r\ninclude '${{THEME}}'\r\nlisten_on unix:/tmp/kitty-slate\r\n\\-personal\r\ninclude {root}\r\n\u{2003}\\-unicode-user/theme.conf\r\n",
        root = root.display()
    );
    let owned = format!(
        "include {}/\r\n  \\theme.conf\r\nlisten_on unix:/tmp/\r\n  \\kitty-slate",
        root.display()
    );
    let input = format!("{user}{owned}");
    assert_eq!(
        output(input.as_bytes(), kitty(&env, input.as_bytes()).unwrap()),
        user.as_bytes()
    );
}

#[test]
fn clean_alacritty_array_positions_forms_and_comment_commas() {
    let env = env();
    let root = env.config_dir().join("managed/alacritty");
    for form in [
        "import = ARRAY\n",
        "[general]\nimport = ARRAY\n",
        "general.import = ARRAY\n",
        "general = { import = ARRAY, live_config_reload = true }\n",
    ] {
        for trailing in [false, true] {
            for mask in 0..16 {
                let mut array = String::from("[\n# start, comment\n");
                let mut retained = Vec::new();
                for index in 0..4 {
                    let owned = mask & (1 << index) != 0;
                    let value = if owned {
                        format!("{}/file{index}.toml", root.display())
                    } else {
                        format!("user{index}.toml")
                    };
                    if !owned {
                        retained.push(value.clone());
                    }
                    // A delimiter on the next line, with commas in comments on
                    // either side, must be located using parser value bounds.
                    array.push_str(&format!("'{value}' # before delimiter, {index}\n"));
                    if index < 3 || trailing {
                        array.push(',');
                    }
                    array.push_str(&format!(" # after delimiter, {index}\n"));
                }
                array.push_str("# end, comment\n]");
                let input = form.replace("ARRAY", &array);
                let bytes = output(input.as_bytes(), alacritty(&env, input.as_bytes()).unwrap());
                let text = std::str::from_utf8(&bytes).unwrap();
                let doc: toml_edit::DocumentMut = text.parse().unwrap();
                let values = doc
                    .get("import")
                    .or_else(|| doc.get("general").and_then(|v| v.get("import")))
                    .unwrap()
                    .as_array()
                    .unwrap();
                assert_eq!(
                    values
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                    retained
                );
                for comment in ["# start, comment", "# end, comment"]
                    .into_iter()
                    .map(str::to_owned)
                    .chain((0..4).flat_map(|i| {
                        [
                            format!("# before delimiter, {i}"),
                            format!("# after delimiter, {i}"),
                        ]
                    }))
                {
                    assert!(text.contains(&comment));
                }
                assert!(matches!(alacritty(&env, &bytes).unwrap(), Edit::Keep(None)));
            }
        }
    }
}

#[test]
fn clean_alacritty_decodes_paths_without_changing_other_values() {
    let env = env();
    let root = env.config_dir().join("managed/alacritty");
    let managed = format!("{}/colors.toml", root.display());
    let escaped = managed.replace('/', "\\u002f");
    let input = format!(
        "import = [\"{escaped}\", 42, {{ nested = '{managed}' }}]\n[general]\nimport = ['''{managed}''', ['{managed}'], '{root}-custom/x', '/mirror{root}/x', '{root}/../user/x', '{root}/$THEME/x']\n",
        root = root.display()
    );
    let expected = input
        .replace(&format!("\"{escaped}\","), "")
        .replace(&format!("'''{managed}''',"), "");
    assert_eq!(
        output(input.as_bytes(), alacritty(&env, input.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}
