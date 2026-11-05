import Foundation

/// A single todo item.
struct Todo: Identifiable, Codable, Equatable {
    let id: UUID
    var title: String
    var done: Bool
    static let maxTitleLength = 120
}

enum Priority: Int, CaseIterable {
    case low = 1
    case medium, high

    var label: String {
        switch self {
        case .low: return "Low"
        case .medium: return "Medium"
        case .high: return "High"
        }
    }
}

extension Todo {
    /// A copy with the done flag flipped.
    func toggled() -> Todo {
        var flipped = self
        flipped.done.toggle()
        return flipped
    }

    static func make(_ title: String) -> Todo {
        Todo(id: UUID(), title: String(title.prefix(maxTitleLength)), done: false)
    }
}
