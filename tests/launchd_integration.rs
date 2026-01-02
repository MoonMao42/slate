use slate_cli::platform::launchd::generate_plist;
use std::io::Cursor;

#[test]
fn test_agent_invocation_path_calls_slate_theme_auto() {
    // Verify that the launchd agent plist is configured to invoke
    // `slate theme --auto` when system appearance changes

    let result = generate_plist("/usr/local/bin/slate");
    assert!(result.is_ok(), "generate_plist should succeed");

    let xml = result.unwrap();

    // Parse the plist to verify structure
    let cursor = Cursor::new(xml.as_bytes());
    let parsed: Result<plist::Value, _> = plist::from_reader(cursor);
    assert!(parsed.is_ok(), "Should be valid plist");

    if let Ok(plist::Value::Dictionary(dict)) = parsed {
        // Verify ProgramArguments contains the theme --auto invocation
        if let Some(plist::Value::Array(args)) = dict.get("ProgramArguments") {
            assert!(args.len() >= 3, "Should have at least 3 arguments: [binary, theme, --auto]");

            // Extract string values
            let arg_strings: Vec<String> = args
                .iter()
                .filter_map(|v| match v {
                    plist::Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .collect();

            // Last two arguments should be "theme" and "--auto"
            assert!(arg_strings.len() >= 3, "Should have string arguments");
            assert_eq!(arg_strings[1], "theme", "Second argument should be 'theme'");
            assert_eq!(arg_strings[2], "--auto", "Third argument should be '--auto'");
        } else {
            panic!("ProgramArguments should be an array");
        }

        // Verify the agent listens to AppleInterfaceThemeChangedNotification
        if let Some(plist::Value::Dictionary(launch_events)) = dict.get("LaunchEvents") {
            assert!(
                launch_events.contains_key("com.apple.notifyd.matching"),
                "Should have notifyd matching configuration"
            );

            // Verify the notification is configured
            if let Some(plist::Value::Dictionary(notifyd)) = launch_events.get("com.apple.notifyd.matching") {
                assert!(
                    notifyd.contains_key("AppleInterfaceThemeChangedNotification"),
                    "Should listen to AppleInterfaceThemeChangedNotification"
                );
            }
        } else {
            panic!("LaunchEvents should be configured");
        }
    } else {
        panic!("Plist should parse to a Dictionary");
    }
}
