type Handler = (request: Request) => Promise<Response>;

const table: Array<{ method: string; pattern: RegExp; handler: Handler }> = [];

function route(method: string, pattern: RegExp, handler: Handler) {
    table.push({ method, pattern, handler });
}

export async function chatSocket(request: Request): Promise<Response> {
    return new Response(request.url);
}

route('GET', /^\/api\/chats\/(?<chatId>[^/]+)\/ws$/, chatSocket);
route('GET', /^\/health$/, async () => new Response('ok'));
