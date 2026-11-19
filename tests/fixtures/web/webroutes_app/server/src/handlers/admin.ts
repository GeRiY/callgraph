export async function removeChat(c: any) {
    return c.json({ removed: true });
}
