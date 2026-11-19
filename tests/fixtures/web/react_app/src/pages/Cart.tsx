import { useCart } from '../hooks/useCart';

export default function Cart() {
    const { cart } = useCart();
    return (
        <ul>
            {cart.items.map((item) => (
                <li key={item}>{item}</li>
            ))}
        </ul>
    );
}
