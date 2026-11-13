//! `callgraph deps`: egy kis Laravel + Vite projekten (composer.json / composer.lock + package.json /
//! package-lock.json) a valószínűleg fölösleges, a hiányzó (nem deklarált JS-import, tranzitív PHP-csomag) és a csak
//! dev függőség; a kivételek (eszköz, auto-discovery, CSS-ből használt, `peerDependencies`, a framework
//! ökoszisztémája, beépített modul), a `.callgraphignore` `deps-ignore` sora, a magyar kimenet, a manifest nélküli
//! projekt. A kilépési kód mindig 0.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Project(PathBuf);

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("callgraph-deps-{id}-{}-{n}", std::process::id()));
        for (path, body) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }
        Self(root.canonicalize().unwrap())
    }

    fn command(&self, args: &[&str], lang: &str, graph: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
        command
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", lang)
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH");
        if !graph {
            command.env("CALLGRAPH_NO_GRAPH", "1");
        }
        command.output().expect("callgraph runs")
    }

    fn run_with(&self, lang: &str, graph: bool) -> String {
        let out = self.command(&["deps"], lang, graph);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn run(&self) -> String {
        self.run_with("en", true)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const COMPOSER_JSON: &str = r#"{
    "name": "acme/shop",
    "require": {
        "php": "^8.2",
        "ext-json": "*",
        "guzzlehttp/guzzle": "^7.9",
        "laravel/framework": "^11.0",
        "spatie/laravel-backup": "^9.0",
        "stripe/stripe-php": "^15.0"
    },
    "require-dev": {
        "fakerphp/faker": "^1.23",
        "laravel/pint": "^1.13",
        "phpunit/phpunit": "^11.0"
    },
    "autoload": {"psr-4": {"App\\": "app/", "Database\\Factories\\": "database/factories/"}},
    "autoload-dev": {"psr-4": {"Tests\\": "tests/"}}
}
"#;

const COMPOSER_LOCK: &str = r#"{
    "packages": [
        {"name": "guzzlehttp/guzzle", "version": "7.9.2", "autoload": {"psr-4": {"GuzzleHttp\\": "src/"}}},
        {"name": "laravel/framework", "version": "v11.31.0", "autoload": {"psr-4": {"Illuminate\\": "src/Illuminate/"}}},
        {"name": "nette/utils", "version": "v4.0.5", "autoload": {"psr-4": {"Nette\\": "src/"}}},
        {"name": "spatie/laravel-backup", "version": "9.1.0", "autoload": {"psr-4": {"Spatie\\Backup\\": "src/"}},
         "extra": {"laravel": {"providers": ["Spatie\\Backup\\BackupServiceProvider"]}}},
        {"name": "stripe/stripe-php", "version": "v15.1.0", "autoload": {"psr-4": {"Stripe\\": "lib/"}}},
        {"name": "symfony/http-foundation", "version": "v7.1.0", "autoload": {"psr-4": {"Symfony\\Component\\HttpFoundation\\": ""}}}
    ],
    "packages-dev": [
        {"name": "fakerphp/faker", "version": "v1.23.1", "autoload": {"psr-4": {"Faker\\": "src/Faker/"}}},
        {"name": "laravel/pint", "version": "v1.18.1", "type": "project"},
        {"name": "phpunit/phpunit", "version": "11.4.0", "autoload": {"classmap": ["src/"]}}
    ]
}
"#;

const PACKAGE_JSON: &str = r#"{
    "private": true,
    "type": "module",
    "dependencies": {"lodash": "^4.17.21"},
    "peerDependencies": {"dayjs": "^1.11.0"},
    "devDependencies": {"@fontsource/inter": "^5.0.0", "vite": "^5.4.0"}
}
"#;

const PACKAGE_LOCK: &str = r#"{
    "name": "shop",
    "lockfileVersion": 3,
    "packages": {
        "": {"name": "shop"},
        "node_modules/@fontsource/inter": {"version": "5.1.0"},
        "node_modules/dayjs": {"version": "1.11.13"},
        "node_modules/lodash": {"version": "4.17.21"},
        "node_modules/vite": {"version": "5.4.10"}
    }
}
"#;

const PAY: &str = r"<?php
namespace App\Services;

use GuzzleHttp\Client;
use Nette\Utils\Html;
use Symfony\Component\HttpFoundation\Response;

class Pay
{
    public function charge(): Response
    {
        $client = new Client();
        $link = Html::el('a');
        return new Response((string) $link);
    }
}
";

const DEMO_DATA: &str = r"<?php
namespace App\Support;

use Faker\Factory;

class DemoData
{
    public function name(): string
    {
        return Factory::create()->name();
    }
}
";

const USER_FACTORY: &str = r"<?php
namespace Database\Factories;

use Faker\Generator;

class UserFactory
{
    public function definition(Generator $faker): array
    {
        return ['name' => $faker->name()];
    }
}
";

const PAY_TEST: &str = r"<?php
namespace Tests\Unit;

use PHPUnit\Framework\TestCase;

class PayTest extends TestCase
{
    public function test_it_pays(): void
    {
        $this->assertTrue(true);
    }
}
";

const APP_JS: &str = r"import axios from 'axios';
import dayjs from 'dayjs';
import { readFileSync } from 'node:fs';

export function load() {
    axios.get('/api/orders');
    return dayjs().format();
}

export function read(path) {
    return readFileSync(path);
}
";

const APP_CSS: &str = r"@import '@fontsource/inter';

body { font-family: Inter, sans-serif; }
";

fn shop(extra: &[(&str, &str)]) -> Project {
    let mut files = vec![
        ("composer.json", COMPOSER_JSON),
        ("composer.lock", COMPOSER_LOCK),
        ("package.json", PACKAGE_JSON),
        ("package-lock.json", PACKAGE_LOCK),
        ("app/Services/Pay.php", PAY),
        ("app/Support/DemoData.php", DEMO_DATA),
        ("database/factories/UserFactory.php", USER_FACTORY),
        ("tests/Unit/PayTest.php", PAY_TEST),
        ("resources/js/app.js", APP_JS),
        ("resources/css/app.css", APP_CSS),
    ];
    files.extend_from_slice(extra);
    Project::new(&files)
}

const EXPECTED: &str = "unused?   stripe/stripe-php  no reference in code\n\
                        unused?   lodash             no reference in code\n\
                        missing   nette/utils        used in app/Services/Pay.php, not in composer.json (transitive)\n\
                        missing   axios              imported in resources/js/app.js, not in package.json\n\
                        dev-only  fakerphp/faker     used in app/Support/DemoData.php\n";

#[test]
fn deps_lists_unused_missing_and_dev_only_one_line_each() {
    let project = shop(&[]);
    // hideg (a gráf most épül), meleg (a projekt-gráfból), és gráf nélkül ugyanaz
    assert_eq!(project.run(), EXPECTED);
    assert_eq!(project.run(), EXPECTED);
    assert_eq!(project.run_with("en", false), EXPECTED);
}

#[test]
fn deps_speaks_hungarian() {
    let project = shop(&[]);
    let text = project.run_with("hu", true);
    assert!(
        text.starts_with("fölösleges?  stripe/stripe-php  nincs hivatkozás a kódban\n"),
        "{text}"
    );
    assert!(
        text.contains(
            "hiányzik     axios              importálva: resources/js/app.js, nincs a package.json-ban\n"
        ),
        "{text}"
    );
    assert!(
        text.ends_with("csak dev     fakerphp/faker     használva: app/Support/DemoData.php\n"),
        "{text}"
    );
}

#[test]
fn deps_ignore_lines_in_callgraphignore_silence_findings() {
    let ignore = "md\ndeps-ignore stripe/*\ndeps-ignore lodash\ndeps-ignore nette/utils\n\
                  deps-ignore axios\ndeps-ignore fakerphp/faker\n";
    let project = shop(&[(".callgraphignore", ignore)]);
    assert_eq!(project.run(), "no dependency issues found\n");
    assert_eq!(project.run_with("hu", true), "nincs függőség-probléma\n");
}

#[test]
fn deps_without_a_manifest_and_wrong_usage() {
    let bare = Project::new(&[("src/a.php", "<?php\nfunction a() {}\n")]);
    assert_eq!(
        bare.run(),
        "deps: no composer.json / package.json in the project\n"
    );
    let wrong = bare.command(&["deps", "guzzle"], "en", true);
    assert_eq!(wrong.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&wrong.stderr).contains("callgraph deps [--project"),
        "{wrong:?}"
    );
}
