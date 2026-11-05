import Foundation

/// Persistence for todos.
protocol TodoStore {
    var name: String { get }
    func fetch(key: String) throws -> [Todo]
    func save(_ todos: [Todo], key: String)
}

actor SyncService {
    private var pending: [Todo] = []

    func enqueue(_ todo: Todo) async {
        pending.append(todo)
    }
}

struct UserDefaultsStore: TodoStore {
    private let defaults = UserDefaults.standard
    let name = "defaults"

    func fetch(key: String) throws -> [Todo] {
        guard let data = defaults.data(forKey: key) else { return [] }
        return try JSONDecoder().decode([Todo].self, from: data)
    }

    func save(_ todos: [Todo], key: String) {
        if let data = try? JSONEncoder().encode(todos) {
            defaults.set(data, forKey: key)
        }
    }
}

func makeDefaultStore() -> TodoStore {
    UserDefaultsStore()
}
