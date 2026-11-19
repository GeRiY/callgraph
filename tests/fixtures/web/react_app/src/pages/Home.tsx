import { Link, useNavigate } from 'react-router-dom';
import { Card } from '../components/Card';
import Badge from '../components/Badge';
import { Button } from '../components/Button';
import Row from '../components/Row';
import { useCart } from '../hooks/useCart';

export default function Home() {
    const navigate = useNavigate();
    const { cart } = useCart();
    const extra = { size: 'sm' };
    return (
        <main>
            <Card title="Cart" onSave={() => navigate('/cart')}>
                <Badge label="new" />
            </Card>
            <Card title="Broken" colour="red" />
            <Card {...extra} onSave={() => navigate('/gone')} />
            <Badge label="x" extra />
            <Button variant="ghost" onClick={() => cart} disabled />
            <Button />
            <Row value={3} />
            <Row label="a" unit="kg" key="r1" data-id="1" />
            <Link to="/cart">cart</Link>
            <Link to="/nowhere">x</Link>
        </main>
    );
}
