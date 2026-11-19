import type { BadgeProps } from '../types/card';

export const Badge = ({ label, tone }: BadgeProps) => <span data-tone={tone}>{label}</span>;

export default Badge;
