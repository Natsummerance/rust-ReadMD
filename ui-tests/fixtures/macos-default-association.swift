// Read-only LaunchServices lookup for explicit synthetic file URLs.
import AppKit
import Foundation

guard CommandLine.arguments.count >= 3 else { exit(2) }
let expected = URL(fileURLWithPath: CommandLine.arguments[1]).resolvingSymlinksInPath().standardizedFileURL
var results: [[String: Any]] = []
for (index, file) in CommandLine.arguments.dropFirst(2).enumerated() {
    let url = URL(fileURLWithPath: file)
    let selected = NSWorkspace.shared.urlForApplication(toOpen: url)
    let matches = selected?.resolvingSymlinksInPath().standardizedFileURL == expected
    results.append(["fixture": index, "fileURLIsLocal": url.isFileURL,
                    "selectedInstalledApplication": matches,
                    "bundleIdentifier": selected.flatMap { Bundle(url: $0)?.bundleIdentifier } ?? "missing"])
}
let passed = results.allSatisfy { $0["selectedInstalledApplication"] as? Bool == true }
let data = try JSONSerialization.data(withJSONObject: ["passed": passed, "associationModified": false, "checks": results], options: [.sortedKeys])
print(String(decoding: data, as: UTF8.self))
exit(passed ? 0 : 1)
