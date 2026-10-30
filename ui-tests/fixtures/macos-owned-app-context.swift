import AppKit
import Foundation

// Only return metadata for this exact, workspace-owned QA bundle. Do not
// expose another application's identity, windows, title or document paths.
let args = CommandLine.arguments
guard args.count == 3, args[1].hasPrefix("asia.readmd.nativeqa.") else { exit(2) }
let expected = URL(fileURLWithPath: args[2]).resolvingSymlinksInPath()
let apps = NSRunningApplication.runningApplications(withBundleIdentifier: args[1])
    .filter { $0.bundleURL?.resolvingSymlinksInPath() == expected }
    .map { ["pid": Int($0.processIdentifier), "active": $0.isActive,
             "finishedLaunching": $0.isFinishedLaunching, "terminated": $0.isTerminated] as [String: Any] }
let data = try JSONSerialization.data(withJSONObject: ["ownedApps": apps], options: [.sortedKeys])
print(String(data: data, encoding: .utf8)!)
