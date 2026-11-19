import { Hono } from 'hono';
import { createMessage } from './handlers/messages';
import * as admin from './handlers/admin';

const app = new Hono();
const chats = new Hono();

export function listChats(c: any) {
    return c.json([]);
}

app.get('/api/chats', listChats);
app.get('/api/chats/:id', (c) => c.json({ id: c.req.param('id') }));
app.post('/api/chats/:id/messages', createMessage);
app.delete('/api/chats/:id', admin.removeChat);

chats.get('/archive', listChats);
app.route('/api/old', chats);

export default app;
