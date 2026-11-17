import type { Handler } from './routing';

export function guarded(handler: Handler, options: { adminOnly?: boolean } = {}): Handler {
    return async (request) => {
        if (options.adminOnly && !request.headers.get('x-admin')) {
            return new Response('forbidden', { status: 403 });
        }
        return handler(request);
    };
}
