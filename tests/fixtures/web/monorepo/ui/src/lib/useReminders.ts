import { isSeason } from '@common/calendar';
import { t } from './messages';
import { matchSeeds } from './seedsearch';

export function remindSeason(code: string): string {
    matchSeeds(code, ['spring', 'summer']);
    return isSeason(code) ? t('overview.title') : code;
}
