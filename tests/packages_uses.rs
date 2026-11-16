//! `callgraph uses <csomag>`: egy kis Laravel-szerű projekten (composer.json + composer.lock, `GuzzleHttp\Client`)
//! az összesítés, a `--calls` lista, a csak típusként használt import (`(type)`), a többértelmű, az ismeretlen és a
//! nem használt csomag; a projekt-gráfból (hideg és meleg indulás) és gráf nélkül ugyanaz a kimenet.

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
            std::env::temp_dir().join(format!("callgraph-uses-{id}-{}-{n}", std::process::id()));
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

    fn run_with(&self, args: &[&str], lang: &str, graph: bool) -> String {
        let out = self.command(args, lang, graph);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn run(&self, args: &[&str]) -> String {
        self.run_with(args, "en", true)
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
        "guzzlehttp/guzzle": "^7.9",
        "guzzlehttp/psr7": "^2.7",
        "laravel/framework": "^11.0",
        "laravel/sanctum": "^4.0"
    },
    "autoload": {"psr-4": {"App\\": "app/"}}
}
"#;

const COMPOSER_LOCK: &str = r#"{
    "packages": [
        {"name": "guzzlehttp/guzzle", "version": "7.9.2", "autoload": {"psr-4": {"GuzzleHttp\\": "src/"}}},
        {"name": "guzzlehttp/psr7", "version": "2.7.0", "autoload": {"psr-4": {"GuzzleHttp\\Psr7\\": "src/"}}},
        {"name": "laravel/framework", "version": "v11.31.0", "autoload": {"psr-4": {"Illuminate\\": "src/Illuminate/"}}},
        {"name": "laravel/sanctum", "version": "v4.0.3", "autoload": {"psr-4": {"Laravel\\Sanctum\\": "src/"}}}
    ],
    "packages-dev": []
}
"#;

const GATEWAY: &str = r"<?php
namespace App\Services;

use GuzzleHttp\Client;
use GuzzleHttp\Exception\RequestException;

class PaymentGateway
{
    private Client $client;

    public function charge(int $amount): array
    {
        try {
            $this->client->request('POST', '/charge');
            $this->client->request('POST', '/capture');
        } catch (RequestException $e) {
            return [];
        }
        return [];
    }

    public function make(): Client
    {
        return new Client();
    }
}
";

const DHL: &str = r"<?php
namespace App\Services\Shipping;

use GuzzleHttp\Client;

class Dhl
{
    public function client(): Client
    {
        return new Client(['base_uri' => 'https://dhl.example']);
    }
}
";

const CONTROLLER: &str = r"<?php
namespace App\Http\Controllers;

use App\Services\PaymentGateway;
use Illuminate\Http\Request;

class OrderController
{
    private PaymentGateway $gateway;

    public function store(Request $request): array
    {
        return $this->gateway->charge(100);
    }
}
";

fn shop() -> Project {
    Project::new(&[
        ("composer.json", COMPOSER_JSON),
        ("composer.lock", COMPOSER_LOCK),
        ("app/Services/PaymentGateway.php", GATEWAY),
        ("app/Services/Shipping/Dhl.php", DHL),
        ("app/Http/Controllers/OrderController.php", CONTROLLER),
    ])
}

#[test]
fn uses_summarises_a_package_per_api_symbol() {
    let project = shop();
    assert_eq!(
        project.run(&["uses", "guzzle"]),
        "guzzlehttp/guzzle 7.9.2 · 4 calls · 1 ref · 2 files\n\
         \x20 Client::request   2  app/Services/PaymentGateway.php\n\
         \x20 new Client        2  app/Services/PaymentGateway.php +1\n\
         \x20 RequestException  1  (type) app/Services/PaymentGateway.php\n"
    );
    // csak típus-hivatkozás (paraméter-típus), hívás nélkül
    assert_eq!(
        project.run(&["uses", "laravel"]),
        "laravel/framework 11.31.0 · 0 calls · 1 ref · 1 file\n\
         \x20 Request  1  (type) app/Http/Controllers/OrderController.php\n"
    );
}

#[test]
fn uses_calls_lists_every_call_and_matches_without_the_graph() {
    let project = shop();
    let expected = "guzzlehttp/guzzle 7.9.2 · 4 calls · 1 ref · 2 files\n\
                    app/Services/PaymentGateway.php\n\
                    \x20 L14 Client::request\n\
                    \x20 L15 Client::request\n\
                    \x20 L24 new Client\n\
                    \x20 (type) RequestException\n\
                    app/Services/Shipping/Dhl.php\n\
                    \x20 L10 new Client\n";
    let args = ["uses", "guzzlehttp/guzzle", "--calls"];
    // hideg (a gráf most épül), meleg (a szűkített tényekből, az importokkal együtt), és gráf nélkül
    assert_eq!(project.run_with(&args, "en", true), expected);
    assert_eq!(project.run_with(&args, "en", true), expected);
    assert_eq!(project.run_with(&args, "en", false), expected);
}

#[test]
fn uses_explains_ambiguous_unknown_and_unused_packages_in_one_line() {
    let project = shop();
    assert_eq!(
        project.run(&["uses", "guzzlehttp"]),
        "ambiguous package name: guzzlehttp → guzzlehttp/guzzle, guzzlehttp/psr7 (give the full name)\n"
    );
    assert_eq!(
        project.run(&["uses", "guzle"]),
        "unknown package: guzle (not in composer.json / package.json or a lock, and no call uses it); closest: guzzlehttp/guzzle\n"
    );
    assert_eq!(
        project.run(&["uses", "sanctum"]),
        "laravel/sanctum 4.0.3: not used in the code (no call or import found)\n"
    );
    assert_eq!(
        project.run_with(&["uses", "sanctum"], "hu", true),
        "laravel/sanctum 4.0.3: a kód nem használja (nincs hívás vagy import)\n"
    );
    // manifest nélkül
    let bare = Project::new(&[("src/a.php", "<?php\nfunction a() {}\n")]);
    assert_eq!(
        bare.run(&["uses", "guzzle"]),
        "unknown package: guzzle (no composer.json / package.json in the project)\n"
    );
    // hibás használat: 2-es kód, a használati sorral
    let wrong = project.command(&["uses"], "en", true);
    assert_eq!(wrong.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&wrong.stderr).contains("callgraph uses <package>"),
        "{wrong:?}"
    );
}
