# Benchmark: GEN-CMS (build a mini CMS with callgraph `gen` only)

## Goal
Build a working mini CMS where **every line of application code is written by `callgraph gen`** (plus `rename` / `undo`).
The task measures how far the generator can carry a realistic project, how many calls it takes, and which gaps
it exposes. It is reusable: repeat it per release and fill one row of the results table.

## Stack
Laravel 11/12, Inertia (server + Vue 3 client), Vue 3, SQLite, Vite.

## Feature spec
- **Pages**: title, slug (unique), body, published_at (nullable).
- **Posts**: title, slug (unique), excerpt, body, published_at; belongs to one Category; many-to-many Tags.
- **Categories**: name, slug (unique). **Tags**: name, slug (unique).
- **Admin area** (`/admin`, prefix + route names `admin.*`): index / create / edit / delete for Pages, Posts,
  Categories, Tags; server-side validation (FormRequests, incl. unique slug ignoring the current record);
  flash messages after create / update / delete, shown in the layout.
- **Public site**: home (latest posts), post list (pagination, search by title/excerpt, filter by category and tag),
  post detail (`/posts/{slug}`), page detail (`/pages/{slug}`); only published records
  (`published_at <= now`) are visible.
- **Seeders and factories** for all models (demo data: categories, tags, posts with tags, pages).
- **Feature tests**: admin CRUD for each resource (incl. validation failures), public home / list / search /
  filters / detail / page, unpublished is 404.

## Rules
Only `callgraph gen` / `rename` / `undo` may create or change application code (models, migrations, controllers,
requests, routes, middleware, Vue pages/components, JS entry/Vite wiring, seeders, factories, tests, config values
in code files).

Allowed infrastructure (not counted as manual edits): `composer create-project`, `composer require`,
`npm install`, `php artisan migrate|migrate:fresh|test|route:list|key:generate|db:seed`, `npm run build`,
creating an empty SQLite file. NOT allowed: `php artisan make:*`, editors, `sed`/`echo >` on project files, copy-paste.
`.env` / config files: if gen cannot edit them that is a generator gap; only pure environment settings
(`.env` values such as `DB_CONNECTION`) may be treated as infra, and the decision must be logged.

Read files only to verify (prefer `callgraph` outline / `--lines`).

## Acceptance
1. `php artisan migrate:fresh --seed` succeeds.
2. `php artisan test` is green (feature tests above included).
3. `npm run build` succeeds.
4. `php artisan route:list` shows all admin resource routes and the public routes.
5. A smoke test renders every public page and every admin index (Inertia component asserted).

## Metrics to record
- number of gen calls (total), failed gen calls (error returned, nothing written)
- manual edits (must be 0)
- generator gaps found, and gaps fixed in the generator (list)
- wall time (start to acceptance), number of check runs (migrate / test / build)
- token notes: rough size of gen outputs and of the expressions sent (lines/characters), any output that was noisy

## Results

| Run | Date | callgraph version | gen calls | failed | manual edits | gaps found | gaps fixed | wall time | acceptance | notes |
|-----|------|-------------------|-----------|--------|--------------|------------|------------|-----------|------------|-------|

Fill one row per run in a local copy; measured results are not committed to the repository.
