import type { ReactNode } from 'react';

interface CardProps {
    title: string;
    size?: 'sm' | 'md';
    onSave: () => void;
    children?: ReactNode;
}

export function Card({ title, size = 'md', onSave, children }: CardProps) {
    return (
        <section className={size}>
            <h2>{title}</h2>
            {children}
            <button onClick={onSave}>save</button>
        </section>
    );
}
