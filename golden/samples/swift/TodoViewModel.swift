import Foundation
import Combine

/// Holds the todo list state for the views.
final class TodoViewModel: ObservableObject {
    @Published private(set) var todos: [Todo] = []
    @Published var errorMessage: String?
    static let storageKey = "todos.v1"
    private let store: TodoStore

    init(store: TodoStore = UserDefaultsStore()) {
        self.store = store
    }

    /// Loads the saved todos.
    func load() {
        do {
            todos = try store.fetch(key: Self.storageKey)
        } catch {
            errorMessage = "Could not load: \(error.localizedDescription)"
            print(error)
        }
    }

    func add(title: String) {
        let todo = Todo(id: UUID(), title: title, done: false)
        todos.append(todo)
        save()
    }

    func visible(showDone: Bool) -> [Todo] {
        if showDone {
            return todos
        } else {
            return todos.filter { !$0.done }
        }
    }

    private func save() {
        DispatchQueue.main.async {
            self.store.save(self.todos, key: Self.storageKey)
        }
    }

    var openCount: Int {
        todos.filter { !$0.done }.count
    }
}
