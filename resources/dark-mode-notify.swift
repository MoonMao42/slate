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
    if CommandLine.arguments.dropFirst().first == "--events" {
        print(isDarkMode() ? "dark" : "light")
        fflush(stdout)
        return
    }
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

// Managed mode is a passive event source. If its Rust owner dies, stop even
// while the desktop is idle; never leave an orphan that can apply themes.
var parentTimer: Timer?
if CommandLine.arguments.dropFirst().first == "--events" {
    guard CommandLine.arguments.count == 3,
          let parent = Int32(CommandLine.arguments[2]), parent > 1 else { exit(2) }
    if getppid() != parent { exit(0) }
    parentTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { _ in
        if getppid() != parent { exit(0) }
    }
}

// Run once on startup to sync state
NSApplication.shared.setActivationPolicy(.prohibited)
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
