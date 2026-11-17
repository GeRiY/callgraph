export type Season = 'spring' | 'summer' | 'autumn' | 'winter';

export function isSeason(value: string): value is Season {
    return ['spring', 'summer', 'autumn', 'winter'].includes(value);
}

export interface BedSummary {
    id: string;
    label: string;
}
