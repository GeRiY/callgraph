export async function listChats() {
    return (await fetch('/api/chats')).json();
}

export async function loadChat(id: string) {
    return (await fetch(`/api/chats/${id}`)).json();
}

export async function sendMessage(id: string, text: string) {
    return fetch(`/api/chats/${id}/messages`, { method: 'POST', body: JSON.stringify({ text }) });
}

export async function loadStats() {
    return (await fetch('/api/stats')).json();
}

export async function loadExternal() {
    return fetch('https://example.com/api/other');
}
