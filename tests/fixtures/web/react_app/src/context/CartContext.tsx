import { createContext, useState, type ReactNode } from 'react';

export interface CartState {
    items: string[];
}

export const CartContext = createContext<CartState>({ items: [] });

export function CartProvider({ children }: { children: ReactNode }) {
    const [items] = useState<string[]>([]);
    return <CartContext.Provider value={{ items }}>{children}</CartContext.Provider>;
}
