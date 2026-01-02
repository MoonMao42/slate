use slate_cli::platform::launchd::generate_plist;
use std::io::Cursor;

#[test]
fn test_generate_plist_produces_valid_xml() {
    let result = generate_plist("/usr/local/bin/slate");
    assert!(result.is_ok(), "generate_plist should succeed");

    let xml = result.unwrap();

    // Verify it's valid XML by checking for plist header
    assert!(xml.contains("<?xml"), "Should contain XML declaration");
    assert!(xml.contains("<plist"), "Should contain plist root element");
    assert!(xml.contains("</plist>"), "Should have closing plist tag");
}

#[test]
fn test_plist_contains_required_keys() {
    let result = generate_plist("/usr/local/bin/slate");
    assert!(result.is_ok());

    let xml = result.unwrap();

    // Required keys for launchd plist
    assert!(xml.contains("<key>Label</key>"), "Should contain Label key");
    assert!(xml.contains("<key>ProgramArguments</key>"), "Should contain ProgramArguments key");
    assert!(xml.contains("<key>LaunchEvents</key>"), "Should contain LaunchEvents key");
    assert!(xml.contains("sh.slate.auto-theme"), "Should contain agent label");
}

#[test]
fn test_plist_program_arguments_contains_binary() {
    let binary_path = "/opt/homebrew/bin/slate";
    let result = generate_plist(binary_path);
    assert!(result.is_ok());

    let xml = result.unwrap();

    // Binary path should appear in ProgramArguments array
    assert!(xml.contains("/opt/homebrew/bin/slate"), "Should contain binary path");
    assert!(xml.contains("<string>theme</string>"), "Should contain theme argument");
    assert!(xml.contains("<string>--auto</string>"), "Should contain --auto argument");
}

#[test]
fn test_plist_notification_key_configured() {
    let result = generate_plist("/usr/local/bin/slate");
    assert!(result.is_ok());

    let xml = result.unwrap();

    // AppleInterfaceThemeChangedNotification should be in LaunchEvents
    assert!(
        xml.contains("AppleInterfaceThemeChangedNotification"),
        "Should listen to appearance change notification"
    );
    assert!(
        xml.contains("com.apple.notifyd.matching"),
        "Should use notifyd matching for events"
    );
}

#[test]
fn test_plist_is_valid_dictionary_structure() {
    // This test validates that the plist can be parsed back by the plist crate
    let result = generate_plist("/usr/local/bin/slate");
    assert!(result.is_ok());

    let xml = result.unwrap();

    // Try to parse the generated XML as a valid plist
    let cursor = Cursor::new(xml.as_bytes());
    let parsed: Result<plist::Value, _> = plist::from_reader(cursor);

    assert!(parsed.is_ok(), "Generated plist should be parseable by plist crate");

    if let Ok(plist::Value::Dictionary(dict)) = parsed {
        // Verify required top-level keys exist
        assert!(dict.contains_key("Label"), "Should have Label key");
        assert!(dict.contains_key("ProgramArguments"), "Should have ProgramArguments key");
        assert!(dict.contains_key("LaunchEvents"), "Should have LaunchEvents key");
    } else {
        panic!("Plist should parse to a Dictionary");
    }
}

#[test]
fn test_plist_launch_events_structure() {
    let result = generate_plist("/usr/local/bin/slate");
    assert!(result.is_ok());

    let xml = result.unwrap();

    // LaunchEvents should be an array or dict with event configurations
    assert!(xml.contains("<key>LaunchEvents</key>"), "Should have LaunchEvents");

    // Check for proper notification matching structure
    let has_matching = xml.contains("com.apple.notifyd.matching");
    assert!(has_matching, "Should have notifyd matching configuration");
}
