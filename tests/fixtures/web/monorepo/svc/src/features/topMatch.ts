import { matchSeeds } from '../../../ui/src/lib/seedsearch';

export function topMatch(prefix: string, names: string[]): string | undefined {
    return matchSeeds(prefix, names)[0];
}
