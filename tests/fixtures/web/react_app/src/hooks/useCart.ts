import { useContext, useState } from 'react';
import { CartContext } from '../context/CartContext';

export function useCart() {
    const cart = useContext(CartContext);
    const [open, setOpen] = useState(false);
    return { cart, open, setOpen };
}
