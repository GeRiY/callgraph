//! A manifest- és lock-stratégiák egységtesztjei, memóriabeli fixture-ökön (fájlrendszer nélkül).

use super::{
    BunLock, Composer, ComposerLock, LockSource, ManifestSource, Npm, NpmLock, PnpmLock, YarnLock,
};
use crate::packages::model::{Dependency, Installed};

fn dep(name: &str, range: &str, dev: bool) -> Dependency {
    Dependency {
        name: name.to_owned(),
        range: range.to_owned(),
        dev,
    }
}

/// (név, verzió) párok a lock-bejegyzésekből.
fn versions(list: &[Installed]) -> Vec<(&str, &str)> {
    list.iter()
        .map(|item| (item.name.as_str(), item.version.as_str()))
        .collect()
}

#[test]
fn composer_json_requires_and_own_namespaces() {
    let manifest = Composer
        .read(
            r#"{
                "name": "acme/shop",
                "require": {"php": "^8.2", "ext-json": "*", "laravel/framework": "^11.0", "lib-icu": "*"},
                "require-dev": {"phpunit/phpunit": "^10.5", "laravel/framework": "^11.0"},
                "autoload": {
                    "psr-4": {"App\\": "app/", "Database\\Factories\\": "database/factories/", "": "src/"},
                    "psr-0": {"Legacy_": "lib/", "Old\\Ns": "old/"}
                },
                "autoload-dev": {"psr-4": {"Tests\\": "tests/", "App\\": "app/"}}
            }"#,
        )
        .unwrap();
    assert_eq!(manifest.name.as_deref(), Some("acme/shop"));
    assert_eq!(
        manifest.deps,
        [
            dep("laravel/framework", "^11.0", false),
            dep("phpunit/phpunit", "^10.5", true)
        ]
    );
    assert_eq!(
        manifest.own_namespaces,
        [
            "App\\",
            "Database\\Factories\\",
            "Legacy_",
            "Old\\Ns\\",
            "Tests\\"
        ]
    );
    assert!(manifest.workspaces.is_empty());
}

#[test]
fn composer_lock_versions_without_v_and_namespaces() {
    let installed = ComposerLock.read(
        r#"{
            "packages": [
                {"name": "laravel/framework", "version": "v11.31.0",
                 "autoload": {"psr-4": {"Illuminate\\": "src/Illuminate/"}, "files": ["src/helpers.php"]}},
                {"name": "acme/fork", "version": "dev-main"},
                {"name": "twig/twig", "version": "v1.44.0", "autoload": {"psr-0": {"Twig_": "lib/"}}}
            ],
            "packages-dev": [
                {"name": "phpunit/phpunit", "version": "10.5.1",
                 "autoload": {"psr-4": {"PHPUnit\\": "src"}}}
            ]
        }"#,
    );
    assert_eq!(
        versions(&installed),
        [
            ("laravel/framework", "11.31.0"),
            ("acme/fork", "dev-main"),
            ("twig/twig", "1.44.0"),
            ("phpunit/phpunit", "10.5.1")
        ]
    );
    assert_eq!(installed[0].namespaces, ["Illuminate\\"]);
    assert!(installed[1].namespaces.is_empty());
    assert_eq!(installed[2].namespaces, ["Twig_"]);
    assert_eq!(installed[3].namespaces, ["PHPUnit\\"]);
}

#[test]
fn package_json_peer_is_not_dev_and_both_workspace_forms() {
    let manifest = Npm
        .read(
            r#"{
                "name": "@acme/web",
                "dependencies": {"vue": "^3.4.0"},
                "peerDependencies": {"pinia": "^2.1", "vue": "^3.0"},
                "devDependencies": {"vite": "^5.0", "typescript": "~5.3.0"},
                "workspaces": ["packages/*", "apps/web"]
            }"#,
        )
        .unwrap();
    assert_eq!(manifest.name.as_deref(), Some("@acme/web"));
    assert_eq!(
        manifest.deps,
        [
            dep("vue", "^3.4.0", false),
            dep("pinia", "^2.1", false),
            dep("vite", "^5.0", true),
            dep("typescript", "~5.3.0", true)
        ]
    );
    assert_eq!(manifest.workspaces, ["packages/*", "apps/web"]);
    let object_form = Npm
        .read(r#"{"workspaces": {"packages": ["modules/*"], "nohoist": ["**/x"]}}"#)
        .unwrap();
    assert_eq!(object_form.workspaces, ["modules/*"]);
    assert!(object_form.deps.is_empty());
}

#[test]
fn package_lock_v1_and_v3() {
    let v3 = NpmLock.read(
        r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "web", "dependencies": {"vue": "^3.4.0"}},
                "node_modules/vue": {"version": "3.4.21"},
                "node_modules/@vue/shared": {"version": "3.4.21"},
                "node_modules/vue/node_modules/semver": {"version": "6.3.1"},
                "packages/ui": {"version": "0.1.0"},
                "node_modules/ui": {"resolved": "packages/ui", "link": true}
            }
        }"#,
    );
    assert_eq!(
        versions(&v3),
        [("vue", "3.4.21"), ("@vue/shared", "3.4.21")]
    );
    let v1 = NpmLock.read(
        r#"{
            "lockfileVersion": 1,
            "dependencies": {
                "vue": {"version": "2.7.16", "dependencies": {"x": {"version": "1.0.0"}}},
                "vuex": {"version": "3.6.2"}
            }
        }"#,
    );
    assert_eq!(versions(&v1), [("vue", "2.7.16"), ("vuex", "3.6.2")]);
}

const YARN_V1: &str = r#"# THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.
# yarn lockfile v1


"@babel/code-frame@^7.0.0", "@babel/code-frame@^7.10.4":
  version "7.12.13"
  resolved "https://registry.yarnpkg.com/@babel/code-frame/-/code-frame-7.12.13.tgz"
  dependencies:
    "@babel/highlight" "^7.12.13"

vue@^2.6.0:
  version "2.6.14"

vue@^3.4.0, vue@^3.4.15:
  version "3.4.21"
  dependencies:
    version "9.9.9"
"#;

const YARN_BERRY: &str = r#"# This file is generated by running "yarn install" inside your project.

__metadata:
  version: 8
  cacheKey: 10c0

"@vue/shared@npm:3.4.21":
  version: 3.4.21
  resolution: "@vue/shared@npm:3.4.21"
  languageName: node
  linkType: hard

"vue@npm:^3.4.0, vue@npm:^3.4.21":
  version: 3.4.21
  resolution: "vue@npm:3.4.21"
  dependencies:
    "@vue/shared": "npm:3.4.21"

"web@workspace:.":
  version: 0.0.0-use.local
  resolution: "web@workspace:."
"#;

#[test]
fn yarn_classic_and_berry() {
    let classic = YarnLock.read(YARN_V1);
    assert_eq!(
        versions(&classic),
        [
            ("@babel/code-frame", "7.12.13"),
            ("vue", "2.6.14"),
            ("vue", "3.4.21")
        ]
    );
    assert_eq!(classic[0].ranges, ["^7.0.0", "^7.10.4"]);
    assert_eq!(classic[2].ranges, ["^3.4.0", "^3.4.15"]);
    let berry = YarnLock.read(YARN_BERRY);
    assert_eq!(
        versions(&berry),
        [
            ("@vue/shared", "3.4.21"),
            ("vue", "3.4.21"),
            ("web", "0.0.0-use.local")
        ]
    );
    assert_eq!(berry[1].ranges, ["^3.4.0", "^3.4.21"]);
    assert_eq!(berry[2].ranges, ["workspace:."]);
}

const PNPM_V9: &str = r"lockfileVersion: '9.0'

settings:
  autoInstallPeers: true

importers:

  .:
    dependencies:
      '@vue/shared':
        specifier: ^3.4.0
        version: 3.4.21
      vue:
        specifier: ^3.4.0
        version: 3.4.21(typescript@5.3.3)
      ui:
        specifier: workspace:*
        version: link:packages/ui
    devDependencies:
      vite:
        specifier: ^5.0.0
        version: 5.1.4(@types/node@20.11.0)

  packages/ui:
    dependencies:
      react:
        specifier: ^18
        version: 18.2.0

packages:

  vue@3.4.21:
    resolution: {integrity: sha512-x}
";

const PNPM_V6: &str = r"lockfileVersion: '6.0'

dependencies:
  vue:
    specifier: ^3.4.0
    version: 3.4.21(typescript@5.3.3)

devDependencies:
  typescript:
    specifier: ~5.3.0
    version: 5.3.3

packages:

  /vue@3.4.21(typescript@5.3.3):
    resolution: {integrity: sha512-x}
    dependencies:
      '@vue/shared': 3.4.21
";

const PNPM_V5: &str = r"lockfileVersion: 5.4

specifiers:
  vue: ^3.2.0
  vite: ^4.0.0

dependencies:
  vue: 3.2.45

devDependencies:
  vite: 4.0.0_@types+node@18.11.9

packages:

  /vue/3.2.45:
    resolution: {integrity: sha512-x}
";

#[test]
fn pnpm_v9_v6_and_the_old_format() {
    assert_eq!(
        versions(&PnpmLock.read(PNPM_V9)),
        [
            ("@vue/shared", "3.4.21"),
            ("vue", "3.4.21"),
            ("vite", "5.1.4")
        ]
    );
    assert_eq!(
        versions(&PnpmLock.read(PNPM_V6)),
        [("vue", "3.4.21"), ("typescript", "5.3.3")]
    );
    assert_eq!(
        versions(&PnpmLock.read(PNPM_V5)),
        [("vue", "3.2.45"), ("vite", "4.0.0")]
    );
}

#[test]
fn bun_lock_is_jsonc_with_trailing_commas() {
    let installed = BunLock.read(
        r#"{
  // bun.lock
  "lockfileVersion": 1,
  "workspaces": {
    "": {
      "name": "web",
      "dependencies": {
        "vue": "^3.4.0",
      },
    },
  },
  "packages": {
    "@vue/shared": ["@vue/shared@3.4.21", "", {}, "sha512-x"],
    "vue": ["vue@3.4.21", "", { "dependencies": { "@vue/shared": "3.4.21" } }, "sha512-y"],
    "vue/semver": ["semver@6.3.1", "", {}, "sha512-z"],
    "ui": ["ui@workspace:packages/ui"],
  },
}
"#,
    );
    assert_eq!(
        versions(&installed),
        [("@vue/shared", "3.4.21"), ("vue", "3.4.21")]
    );
}

#[test]
fn broken_files_are_empty() {
    assert!(Composer.read("not json").is_none());
    assert!(Composer.read("[1, 2]").is_none());
    assert!(Npm.read("{").is_none());
    let locks: [&dyn LockSource; 5] = [&ComposerLock, &NpmLock, &YarnLock, &PnpmLock, &BunLock];
    for lock in locks {
        for text in ["", "{", "garbage: [", "\u{0}\u{1}binary", "[]"] {
            assert!(lock.read(text).is_empty(), "{}: {text:?}", lock.file());
        }
    }
}
