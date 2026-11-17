export type Handler = (request: Request) => Promise<Response>;

export interface RouteEntry {
    method: string;
    pattern: RegExp;
    handler: Handler;
}

export function addRoute(method: string, pattern: RegExp, handler: Handler): RouteEntry {
    return { method, pattern, handler };
}
