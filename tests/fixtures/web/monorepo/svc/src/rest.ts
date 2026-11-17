import { isSeason } from '@common/calendar';

export async function streamSensor(request: Request): Promise<Response> {
    const season = new URL(request.url).searchParams.get('season') ?? 'spring';
    return new Response(isSeason(season) ? 'ok' : 'bad season');
}

export async function findPlants(request: Request): Promise<Response> {
    return new Response(new URL(request.url).search);
}

export async function listBeds(): Promise<Response> {
    return new Response('[]');
}
