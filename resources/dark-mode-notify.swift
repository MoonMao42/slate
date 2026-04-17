import Cocoa

// Minimal event-driven dark mode watcher.
// Listens for AppleInterfaceThemeChangedNotification via DistributedNotificationCenter,
// then executes the provided command with DARKMODE=1|0 environment variable.
// Usage: dark-mode-notify <command> [args...]
// Derived from https://github.com/bouk/dark-mode-notify (MIT)

func isDarkMode() -> Bool {
    UserDefaults.standard.string(forKey: "AppleInterfaceStyle") == "Dark"
}

func runCommand() {
    let args = Array(CommandLine.arguments.dropFirst())
    guard !args.isEmpty else { return }

    let task = Process()
    task.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    task.arguments = args
    var env = ProcessInfo.processInfo.environment
    env["DARKMODE"] = isDarkMode() ? "1" : "0"
    task.environment = env

    try? task.run()
    task.waitUntilExit()
}

// Run once on startup to sync state
runCommand()

// Listen for appearance changes
DistributedNotificationCenter.default.addObserver(
    forName: NSNotification.Name("AppleInterfaceThemeChangedNotification"),
    object: nil,
    queue: .main
) { _ in
    runCommand()
}

// Listen for wake from sleep (appearance may have changed via schedule)
NSWorkspace.shared.notificationCenter.addObserver(
    forName: NSWorkspace.screensDidWakeNotification,
    object: nil,
    queue: .main
) { _ in
    runCommand()
}

NSApplication.shared.run()
