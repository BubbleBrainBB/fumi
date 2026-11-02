import Foundation

// Standalone test runner for LibraryList (the sidebar's libraries, design
// board 60 B):
//
//   swiftc Fumi/Services/LibraryList.swift Tests/LibraryListTests.swift -o /tmp/library-tests && /tmp/library-tests

@main
enum LibraryListTests {
    static var failures = 0

    static func check(_ condition: Bool, _ message: String, line: Int = #line) {
        if !condition {
            failures += 1
            print("FAIL (line \(line)): \(message)")
        }
    }

    static func main() {
        let suite = "fumi.libraryListTests"
        let defaults = UserDefaults(suiteName: suite)!
        defaults.removePersistentDomain(forName: suite)

        // MARK: Seeding and adding
        let list = LibraryList(defaults: defaults, seed: { ["/a/Garden", "/b/Work", "/a/Garden"] })
        check(list.entries.map(\.path) == ["/a/Garden", "/b/Work"], "seeded once each: \(list.entries.map(\.path))")
        list.add(URL(fileURLWithPath: "/c/Reading"))
        list.add(URL(fileURLWithPath: "/b/Work/"))
        check(list.entries.map(\.path) == ["/a/Garden", "/b/Work", "/c/Reading"], "new ones last, no repeats")

        // MARK: Order, names, icons
        list.move("/c/Reading", to: "/a/Garden")
        check(list.entries.map(\.path) == ["/c/Reading", "/a/Garden", "/b/Work"], "dropped on the first row")
        list.move("/c/Reading", to: "/b/Work")
        check(list.entries.map(\.path) == ["/a/Garden", "/b/Work", "/c/Reading"], "dropped on the last row")

        list.rename("/b/Work", to: "  Work Notes ")
        check(list.entry(for: URL(fileURLWithPath: "/b/Work"))?.displayName == "Work Notes", "renamed")
        let tint = list.entries[1].tint
        list.rename("/b/Work", to: "")
        check(list.entries[1].displayName == "Work" && list.entries[1].tint == tint, "back to the folder's name, same tint")
        list.setIcon("/a/Garden", to: "📚 books")
        check(list.entries[0].icon == "📚", "one emoji")
        list.setIcon("/a/Garden", to: " ")
        check(list.entries[0].icon == nil, "empty: back to the plain icon")
        list.remove("/c/Reading")

        // Saved: a new list reads the same back, and ignores the seed.
        let reread = LibraryList(defaults: defaults, seed: { ["/x"] })
        check(reread.entries == list.entries, "saved")

        // MARK: Locations
        let home = "/Users/me"
        check(LibraryList.location(of: URL(fileURLWithPath: "/Users/me/Documents/Garden"), home: home) == "~/Documents", "home")
        check(LibraryList.location(of: URL(fileURLWithPath: "/Users/me/Garden"), home: home) == "~", "home itself")
        check(LibraryList.location(of: URL(fileURLWithPath: "/Users/me/Library/Mobile Documents/com~apple~CloudDocs/Reading"), home: home)
              == "iCloud Drive", "iCloud Drive")
        check(LibraryList.location(of: URL(fileURLWithPath: "/Volumes/T7/Notes/Archive"), home: home) == "T7/Notes", "a disk")

        // MARK: Missing
        let none: (String) -> Bool = { _ in false }
        check(LibraryList.missingReason(URL(fileURLWithPath: "/Volumes/T7/Archive"), exists: none) == "Disk “T7” isn’t connected", "disk")
        check(LibraryList.missingReason(URL(fileURLWithPath: "/Users/me/Gone"), exists: none) == "Moved or deleted", "gone")
        check(LibraryList.missingReason(URL(fileURLWithPath: "/tmp")) == nil, "there")

        defaults.removePersistentDomain(forName: suite)
        if failures == 0 {
            print("All LibraryList tests passed")
        } else {
            print("\(failures) failure(s)")
            exit(1)
        }
    }
}
