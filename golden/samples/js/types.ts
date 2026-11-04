/** Shared shapes for the sample module. */
export interface Item {
  id: number;
  label: string;
}

export enum Mode {
  Fast = 'fast',
  Slow = 'slow',
}

export const MAX_ITEMS = 10;

export function pick<T extends Item>(items: T[], id: number): T | undefined {
  // TODO: replace the linear search
  return items.find((i) => i.id === id);
}

export class Store<T> {
  private items: T[] = [];
  add(item: T): void {
    this.items.push(item);
  }
  static create<U>(): Store<U> {
    return new Store<U>();
  }
}
