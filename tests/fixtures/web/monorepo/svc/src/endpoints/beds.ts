import * as rest from '../rest';
import { guarded } from '../core/guard';
import { addRoute } from '../core/routing';

export const bedRoutes = [
    addRoute('GET', /^\/beds$/, guarded(rest.listBeds)),
    addRoute('GET', /^\/beds\/(?<bedId>[^/]+)\/sensor$/, rest.streamSensor),
    addRoute('GET', /^\/plants\/find$/, guarded(rest.findPlants, { adminOnly: true })),
];
