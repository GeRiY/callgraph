import { Routes, Route } from 'react-router-dom';
import Home from './pages/Home';
import Cart from './pages/Cart';
import { CartProvider } from './context/CartContext';

export function App() {
    return (
        <CartProvider>
            <Routes>
                <Route path="/" element={<Home />} />
                <Route path="/cart" element={<Cart />}>
                    <Route path=":lineId" element={<Cart />} />
                </Route>
            </Routes>
        </CartProvider>
    );
}
