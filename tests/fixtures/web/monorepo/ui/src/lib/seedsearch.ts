export async function loadChart(): Promise<unknown> {
    const charts = await import('chart.js');
    return charts.Chart;
}

export function matchSeeds(prefix: string, names: string[]): string[] {
    return names.filter((name) => name.startsWith(prefix)).slice(0, 5);
}
