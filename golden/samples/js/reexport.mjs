export { pick, Mode } from './types.ts';
export * from './helpers.js';

const registry = new Map();

export const register = (name, fn) => {
  registry.set(name, fn);
};

export default function boot() {
  register('boot', () => 'ready');
  return registry.size;
}
