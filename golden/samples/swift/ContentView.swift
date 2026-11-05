import SwiftUI

/// The main list screen: shows the todos and lets the user add one.
struct ContentView: View {
    @StateObject private var viewModel = TodoViewModel()
    @State private var newTitle: String = ""
    @State private var showDone = false
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject var settings: AppSettings

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    TextField("New todo", text: $newTitle)
                        .textFieldStyle(.roundedBorder)
                    Button("Add") {
                        addTodo()
                    }
                    .disabled(newTitle.isEmpty)
                }
                .padding()
                List(viewModel.visible(showDone: showDone)) { todo in
                    TodoRow(todo: todo)
                }
            }
            .navigationTitle("Todos")
            .onAppear {
                viewModel.load()
            }
        }
    }

    private func addTodo() {
        // TODO: validate the title length before adding
        guard !newTitle.isEmpty else { return }
        viewModel.add(title: newTitle)
        newTitle = ""
    }
}

struct TodoRow: View {
    @Binding var isOn: Bool
    let todo: Todo

    var body: some View {
        Text(todo.title.uppercased())
            .font(.headline)
            .foregroundColor(todo.done ? .gray : .primary)
    }
}
