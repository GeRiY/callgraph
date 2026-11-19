import type { ButtonHTMLAttributes } from 'react';

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
    variant: 'primary' | 'ghost';
}

export function Button({ variant, ...rest }: ButtonProps) {
    return <button className={variant} {...rest} />;
}
