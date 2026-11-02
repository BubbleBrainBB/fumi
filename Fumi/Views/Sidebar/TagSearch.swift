import Foundation

/// Filtering for the sidebar tag search. Pure (no SwiftUI) so it can be unit
/// tested. When searching, the hierarchy is flattened and every tag whose full
/// path matches is shown as a flat row, so a nested match (e.g. `AI/编程`) is
/// found even when its parent doesn't match the query.
enum TagSearch {
    /// All nodes in the tree, parents and descendants, in a single list.
    static func flatten(_ nodes: [TagNode]) -> [TagNode] {
        nodes.flatMap { [$0] + flatten($0.children) }
    }

    /// Tags whose full path contains `query` (case-insensitive), sorted by path.
    /// An empty/whitespace query returns an empty list (caller shows the tree).
    static func matches(in tree: [TagNode], query: String) -> [TagNode] {
        let trimmed = query.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return [] }
        return flatten(tree)
            .filter { $0.id.localizedCaseInsensitiveContains(trimmed) }
            .sorted { $0.id < $1.id }
    }
}
