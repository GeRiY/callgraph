import { Link } from '@inertiajs/react';
import Stamp from '../../Components/Stamp';

interface Props {
    event: { id: number; title: string };
    can: boolean;
    stale: string;
}

export default function Show({ event, can, stale }: Props) {
    return (
        <div>
            <h1>{event.title}</h1>
            <Stamp label={event.title} />
            <Stamp />
            {can && <Link href={route('events.show', event.id)}>again {stale}</Link>}
        </div>
    );
}
