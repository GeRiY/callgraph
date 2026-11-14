//! A Vue-generátor egységtesztjei, memóriabeli fájlrendszerrel és rögzített órával (mint a PHP-teszteknél).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use super::{VueGenerator, wants_vue};
use crate::generate::fs::{Clock, DirEntry, Env, FileSystem};
use crate::generate::generator::{GenRequest, Output, ReplaceAt, Target};

const ROOT: &str = "/p";

#[derive(Default)]
struct MemFs {
    files: RefCell<BTreeMap<PathBuf, String>>,
}

impl MemFs {
    fn with(files: &[(&str, &str)]) -> Self {
        let fs = Self::default();
        for (path, text) in files {
            fs.files
                .borrow_mut()
                .insert(Path::new(ROOT).join(path), (*text).to_owned());
        }
        fs
    }

    fn get(&self, rel: &str) -> Option<String> {
        self.files.borrow().get(&Path::new(ROOT).join(rel)).cloned()
    }

    fn has_backup_of(&self, rel: &str) -> bool {
        let prefix = Path::new(ROOT).join(".callgraph/backups").join(rel);
        self.files
            .borrow()
            .keys()
            .any(|p| p.to_string_lossy().starts_with(&*prefix.to_string_lossy()))
    }
}

impl FileSystem for MemFs {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn write(&self, path: &Path, content: &str) -> io::Result<()> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), content.to_owned());
        Ok(())
    }

    fn create_new(&self, path: &Path, content: &str) -> io::Result<()> {
        if self.files.borrow().contains_key(path) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        self.write(path, content)
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.borrow().contains_key(path)
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        let mut entries: BTreeMap<PathBuf, bool> = BTreeMap::new();
        for path in self.files.borrow().keys() {
            let Ok(rest) = path.strip_prefix(dir) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(first) = parts.next() else {
                continue;
            };
            let is_dir = parts.next().is_some();
            entries.insert(dir.join(first.as_os_str()), is_dir);
        }
        if entries.is_empty() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        Ok(entries
            .into_iter()
            .map(|(path, is_dir)| DirEntry { path, is_dir })
            .collect())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.files.borrow_mut().remove(path);
        Ok(())
    }
}

struct FixedClock(Cell<u64>);

impl Clock for FixedClock {
    fn now_secs(&self) -> u64 {
        self.0.get()
    }
}

const T0: u64 = 1_700_000_000;

fn go(fs: &MemFs, expr: &str, dry: bool, target: Target) -> Result<Output, String> {
    let clock = FixedClock(Cell::new(T0));
    let env = Env {
        root: Path::new(ROOT),
        fs,
        clock: &clock,
    };
    let request = GenRequest {
        expr: expr.to_owned(),
        dry,
        target,
    };
    VueGenerator::new(env)
        .run(&request)
        .map_err(|e| e.to_string())
}

fn write(file: &str) -> Target {
    Target::Write {
        file: file.to_owned(),
    }
}

fn into(file: &str, after: Option<&str>) -> Target {
    Target::Into {
        file: file.to_owned(),
        class: None,
        after: after.map(str::to_owned),
    }
}

fn text(out: &Output) -> String {
    out.lines.join("\n")
}

const TS_PROJECT: &[(&str, &str)] = &[("tsconfig.json", "{}")];

const FULL_SFC: &str = "vue:Card>(script>props(title:str,items:list<Item>,size?:int=3)+emits(save:int,close)+ref:count:int=0+computed:total:int{ items.length }+fn:submit(e:Event):void{ emit('save', count.value) }+form(name,email))+(template>div.card#main>h1{{ title }}+button[@click=submit type=button]{Save})+(style>.card{padding:1rem;gap:.5rem}+.card h1{margin:0})";

#[test]
fn sfc_with_all_sections_in_typescript() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(&fs, FULL_SFC, true, write("src/Card.vue")).unwrap();
    let expected = "\
<script setup lang=\"ts\">
import { computed, ref } from 'vue';
import { useForm } from '@inertiajs/vue3';

const props = withDefaults(defineProps<{
  title: string;
  items: Item[];
  size?: number;
}>(), {
  size: 3,
});

const emit = defineEmits<{
  save: [payload: number];
  close: [];
}>();

const count = ref<number>(0);

const total = computed<number>(() => items.length);

function submit(e: Event): void {
  emit('save', count.value)
}

const form = useForm({
  name: '',
  email: '',
});
</script>

<template>
  <div class=\"card\" id=\"main\">
    <h1>{{ title }}</h1>
    <button @click=\"submit\" type=\"button\">Save</button>
  </div>
</template>

<style scoped>
.card {
  padding: 1rem;
  gap: .5rem;
}

.card h1 {
  margin: 0;
}
</style>";
    assert_eq!(text(&out), expected);
}

#[test]
fn write_creates_the_file_and_never_overwrites() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(&fs, FULL_SFC, false, write("src/Card.vue")).unwrap();
    assert!(out.lines[0].starts_with("wrote src/Card.vue"), "{out:?}");
    assert!(
        fs.get("src/Card.vue")
            .unwrap()
            .contains("<script setup lang=\"ts\">")
    );
    let again = go(&fs, FULL_SFC, false, write("src/Card.vue")).unwrap_err();
    assert!(again.contains("already exists"), "{again}");
}

#[test]
fn sfc_in_plain_javascript() {
    let fs = MemFs::default();
    let expr = "vue:Card>(script>props(title:str,size?:int=3,items:list<Item>=[])+emits(save:int,close)+ref:count:int+fn:go(n?:int):void{ count.value = n })+(template>p{hi})";
    let out = go(&fs, expr, true, write("src/Card.vue")).unwrap();
    let code = text(&out);
    assert!(
        code.starts_with("<script setup>\nimport { ref } from 'vue';\n"),
        "{code}"
    );
    assert!(code.contains("const props = defineProps({\n  title: { type: String, required: true },\n  size: { type: Number, default: 3 },\n  items: { type: Array, default: () => [] },\n});"), "{code}");
    assert!(
        code.contains("const emit = defineEmits(['save', 'close']);"),
        "{code}"
    );
    assert!(code.contains("const count = ref(0);"), "{code}");
    assert!(
        code.contains("function go(n) {\n  count.value = n\n}"),
        "{code}"
    );
    assert!(
        code.contains("<template>\n  <p>hi</p>\n</template>"),
        "{code}"
    );
}

#[test]
fn ts_is_detected_from_an_existing_vue_file_and_semicolons_follow_the_project() {
    let fs = MemFs::with(&[(
        "src/Old.vue",
        "<script setup lang=\"ts\">\nimport { ref } from 'vue'\nconst a = ref(1)\n</script>\n",
    )]);
    let out = go(&fs, "vue:N>(script>ref:n:int=1)", true, write("src/N.vue")).unwrap();
    let code = text(&out);
    assert!(code.contains("<script setup lang=\"ts\">"), "{code}");
    assert!(code.contains("import { ref } from 'vue'\n"), "{code}");
    assert!(code.contains("const n = ref<number>(1)\n"), "{code}");
}

#[test]
fn form_fields_get_defaults_from_their_types() {
    let fs = MemFs::default();
    let out = go(
        &fs,
        "vue:F>(script>form:login(email,remember:bool=true,age:int))",
        true,
        write("src/F.vue"),
    )
    .unwrap();
    let code = text(&out);
    assert!(
        code.contains("const login = useForm({\n  email: '',\n  remember: true,\n  age: 0,\n});"),
        "{code}"
    );
    assert!(
        code.contains("import { useForm } from '@inertiajs/vue3';"),
        "{code}"
    );
}

#[test]
fn template_emmet_with_directives_text_and_repetition() {
    let fs = MemFs::default();
    let expr = "template>div.card#main>h1{{ title }}+ul>li[v-for=\"item in items\" :key=item.id]{{ item.name }}^button[@click=submit type=button]{Save}";
    let out = go(&fs, expr, true, Target::Preview).unwrap();
    let expected = "\
<template>
  <div class=\"card\" id=\"main\">
    <h1>{{ title }}</h1>
    <ul>
      <li v-for=\"item in items\" :key=\"item.id\">{{ item.name }}</li>
    </ul>
    <button @click=\"submit\" type=\"button\">Save</button>
  </div>
</template>";
    assert_eq!(text(&out), expected);

    let rep = go(
        &fs,
        "template>ul.list>li.item$*3{Row $}",
        true,
        Target::Preview,
    )
    .unwrap();
    let expected = "\
<template>
  <ul class=\"list\">
    <li class=\"item1\">Row 1</li>
    <li class=\"item2\">Row 2</li>
    <li class=\"item3\">Row 3</li>
  </ul>
</template>";
    assert_eq!(text(&rep), expected);
}

#[test]
fn template_text_with_apostrophe_void_elements_and_empty_elements() {
    let fs = MemFs::default();
    let out = go(
        &fs,
        "template>div>p{Don't panic}+input[type=text v-model=form.name]+span.x",
        true,
        Target::Preview,
    )
    .unwrap();
    let code = text(&out);
    assert!(code.contains("<p>Don't panic</p>"), "{code}");
    assert!(
        code.contains("<input type=\"text\" v-model=\"form.name\" />"),
        "{code}"
    );
    assert!(code.contains("<span class=\"x\"></span>"), "{code}");
}

#[test]
fn components_are_imported_when_the_project_has_them() {
    let fs = MemFs::with(&[(
        "resources/js/Components/Card.vue",
        "<template><div/></template>\n",
    )]);
    let out = go(
        &fs,
        "vue:Page>(template>div>Card[:title=title]+Missing+Link[href=x]{Go})",
        true,
        write("resources/js/Pages/Page.vue"),
    )
    .unwrap();
    let code = text(&out);
    assert!(
        code.starts_with(
            "<script setup>\nimport { Link } from '@inertiajs/vue3';\nimport Card from '@/Components/Card.vue';\n</script>"
        ),
        "{code}"
    );
    assert!(code.contains("    <Card :title=\"title\" />"), "{code}");
    assert!(code.contains("<Link href=\"x\">Go</Link>"), "{code}");
    assert_eq!(out.unresolved, vec!["Missing".to_owned()]);
}

#[test]
fn explicit_use_directives() {
    let fs = MemFs::default();
    let out = go(
        &fs,
        "vue:U>(script>use:@/Components/Card,vue(onMounted)+use:axios)",
        true,
        write("src/U.vue"),
    )
    .unwrap();
    let expected = "\
<script setup>
import { onMounted } from 'vue';
import Card from '@/Components/Card.vue';
import axios from 'axios';
</script>";
    assert_eq!(text(&out), expected);
}

#[test]
fn style_scoped_by_default_and_global_on_request() {
    let fs = MemFs::default();
    let out = go(
        &fs,
        "vue:S>(style>.a{color:red}+a:hover{x:y})",
        true,
        write("src/S.vue"),
    )
    .unwrap();
    assert_eq!(
        text(&out),
        "<style scoped>\n.a {\n  color: red;\n}\n\na:hover {\n  x: y;\n}\n</style>"
    );
    let out = go(&fs, "style:global>.a{color:red}", true, Target::Preview).unwrap();
    assert_eq!(text(&out), "<style>\n.a {\n  color: red;\n}\n</style>");
}

#[test]
fn composable_file_in_typescript_and_javascript() {
    let fs = MemFs::default();
    let expr = "comp:useCart>ref:items:list<Item>=[]+computed:total:int{ items.value.length }+fn:add(item:Item):void{ items.value.push(item) }";
    let out = go(&fs, expr, true, write("src/composables/useCart.ts")).unwrap();
    let expected = "\
import { computed, ref } from 'vue';

export function useCart() {
  const items = ref<Item[]>([]);

  const total = computed<number>(() => items.value.length);

  function add(item: Item): void {
    items.value.push(item)
  }

  return {
    items,
    total,
    add,
  };
}";
    assert_eq!(text(&out), expected);

    let js = go(&fs, expr, false, write("src/composables/useCart.js")).unwrap();
    assert!(js.lines[0].starts_with("wrote src/composables/useCart.js"));
    let file = fs.get("src/composables/useCart.js").unwrap();
    assert!(file.contains("const items = ref([]);"), "{file}");
    assert!(file.contains("function add(item) {"), "{file}");
}

const EXISTING_SFC: &str = "\
<script setup lang=\"ts\">
import { ref } from 'vue';

const a = ref<number>(1);
</script>

<template>
  <div>
    <p>x</p>
  </div>
</template>
";

#[test]
fn into_script_merges_imports_and_makes_a_backup() {
    let fs = MemFs::with(&[("src/A.vue", EXISTING_SFC)]);
    let out = go(
        &fs,
        "script>computed:b:int{ a.value + 1 }+fn:reset():void{ a.value = 0 }",
        false,
        into("src/A.vue", None),
    )
    .unwrap();
    assert!(
        out.lines[0].starts_with("inserted into src/A.vue: script L"),
        "{out:?}"
    );
    let file = fs.get("src/A.vue").unwrap();
    let expected_head = "\
<script setup lang=\"ts\">
import { ref, computed } from 'vue';

const a = ref<number>(1);

const b = computed<number>(() => a.value + 1);

function reset(): void {
  a.value = 0
}
</script>
";
    assert!(file.starts_with(expected_head), "{file}");
    assert!(fs.has_backup_of("src/A.vue"));
    // másodszor ugyanaz a név: elutasítva
    let again = go(
        &fs,
        "script>fn:reset():void{}",
        false,
        into("src/A.vue", None),
    )
    .unwrap_err();
    assert!(again.contains("already declared"), "{again}");
}

/// A 4 szóközzel behúzott fájlba a beszúrt kód is 4 szóközzel kerül.
#[test]
fn into_a_four_space_file_keeps_its_indent() {
    let sfc = "<script setup>\nconst submit = () => {\n    go();\n};\n</script>\n\n<template>\n    <div>\n        <p>x</p>\n    </div>\n</template>\n";
    let fs = MemFs::with(&[("src/B.vue", sfc)]);
    go(
        &fs,
        "script>fn:reset(){ if (a) {\n  b()\n} }",
        false,
        into("src/B.vue", None),
    )
    .unwrap();
    let file = fs.get("src/B.vue").unwrap();
    assert!(
        file.contains("function reset() {\n    if (a) {\n"),
        "{file}"
    );
    assert!(!file.contains("\n  if (a)"), "{file}");
}

#[test]
fn indent_unit_and_reindent() {
    use super::edit::{indent_unit, reindent};
    let four: Vec<String> = ["a", "    b", "        c"].map(String::from).to_vec();
    assert_eq!(indent_unit(&four), 4);
    let two: Vec<String> = ["a", "  b"].map(String::from).to_vec();
    assert_eq!(indent_unit(&two), 2);
    assert_eq!(indent_unit(&[]), 2);
    let block: Vec<String> = ["x {", "  y", "    z", "", "}"].map(String::from).to_vec();
    assert_eq!(
        reindent(&block, 4),
        ["x {", "    y", "        z", "", "}"]
            .map(String::from)
            .to_vec()
    );
    assert_eq!(reindent(&block, 2), block);
}

#[test]
fn into_script_after_a_named_statement() {
    let fs = MemFs::with(&[("src/A.vue", EXISTING_SFC)]);
    go(
        &fs,
        "script>const:K:int=5",
        false,
        into("src/A.vue", Some("a")),
    )
    .unwrap();
    let file = fs.get("src/A.vue").unwrap();
    assert!(
        file.contains("const a = ref<number>(1);\n\nconst K: number = 5;\n</script>"),
        "{file}"
    );
}

#[test]
fn into_template_after_a_line_or_before_the_root_end() {
    let fs = MemFs::with(&[("src/A.vue", EXISTING_SFC)]);
    go(
        &fs,
        "template>span{y}",
        false,
        into("src/A.vue", Some("L9")),
    )
    .unwrap();
    let file = fs.get("src/A.vue").unwrap();
    assert!(
        file.contains("    <p>x</p>\n    <span>y</span>\n  </div>"),
        "{file}"
    );

    let fs = MemFs::with(&[("src/A.vue", EXISTING_SFC)]);
    go(&fs, "template>span{y}", false, into("src/A.vue", None)).unwrap();
    let file = fs.get("src/A.vue").unwrap();
    assert!(
        file.contains("    <p>x</p>\n    <span>y</span>\n  </div>"),
        "{file}"
    );

    // az nyitó elem sora után: első gyerek
    let fs = MemFs::with(&[("src/A.vue", EXISTING_SFC)]);
    go(
        &fs,
        "template>span{y}",
        false,
        into("src/A.vue", Some("L8")),
    )
    .unwrap();
    let file = fs.get("src/A.vue").unwrap();
    assert!(
        file.contains("  <div>\n    <span>y</span>\n    <p>x</p>"),
        "{file}"
    );
}

#[test]
fn into_template_imports_the_component_into_the_script() {
    let fs = MemFs::with(&[
        ("src/A.vue", EXISTING_SFC),
        ("src/components/Badge.vue", "<template><b/></template>\n"),
    ]);
    go(&fs, "template>Badge", false, into("src/A.vue", None)).unwrap();
    let file = fs.get("src/A.vue").unwrap();
    assert!(
        file.contains("import Badge from '@/components/Badge.vue';"),
        "{file}"
    );
    assert!(file.contains("    <Badge />\n  </div>"), "{file}");
}

#[test]
fn into_style_creates_the_block_or_appends_to_it() {
    let fs = MemFs::with(&[("src/A.vue", EXISTING_SFC)]);
    go(&fs, "style>.a{color:red}", false, into("src/A.vue", None)).unwrap();
    let file = fs.get("src/A.vue").unwrap();
    assert!(
        file.ends_with("</template>\n\n<style scoped>\n.a {\n  color: red;\n}\n</style>\n"),
        "{file}"
    );
    go(&fs, "style>.b{margin:0}", false, into("src/A.vue", None)).unwrap();
    let file = fs.get("src/A.vue").unwrap();
    assert!(
        file.ends_with(".a {\n  color: red;\n}\n\n.b {\n  margin: 0;\n}\n</style>\n"),
        "{file}"
    );
}

#[test]
fn into_creates_a_missing_script_block() {
    let fs = MemFs::with(&[("src/B.vue", "<template>\n  <div></div>\n</template>\n")]);
    go(&fs, "script>ref:n:int=1", false, into("src/B.vue", None)).unwrap();
    let file = fs.get("src/B.vue").unwrap();
    assert!(file.starts_with("<script setup>\nimport { ref } from 'vue';\n\nconst n = ref(1);\n</script>\n\n<template>"), "{file}");
}

const EXISTING_COMPOSABLE: &str = "\
import { ref } from 'vue';

export function useCart() {
  const items = ref<Item[]>([]);

  return {
    items,
  };
}
";

#[test]
fn into_composable_extends_the_return_object() {
    let fs = MemFs::with(&[("src/useCart.ts", EXISTING_COMPOSABLE)]);
    go(
        &fs,
        "ref:count:int=0+fn:clear():void{ items.value = [] }",
        false,
        into("src/useCart.ts", None),
    )
    .unwrap();
    let expected = "\
import { ref } from 'vue';

export function useCart() {
  const items = ref<Item[]>([]);

  const count = ref<number>(0);

  function clear(): void {
    items.value = []
  }

  return {
    items,
    count,
    clear,
  };
}
";
    assert_eq!(fs.get("src/useCart.ts").unwrap(), expected);
}

#[test]
fn into_composable_with_a_one_line_return() {
    let fs = MemFs::with(&[(
        "src/useX.js",
        "import { ref } from 'vue'\n\nexport function useX() {\n  const a = ref(1)\n  return { a }\n}\n",
    )]);
    go(
        &fs,
        "computed:b{ a.value * 2 }",
        false,
        into("src/useX.js", None),
    )
    .unwrap();
    let file = fs.get("src/useX.js").unwrap();
    assert!(file.contains("return { a, b }"), "{file}");
    assert!(
        file.contains("import { ref, computed } from 'vue'"),
        "{file}"
    );
    assert!(
        file.contains("const b = computed(() => a.value * 2)\n"),
        "{file}"
    );
}

#[test]
fn replace_a_function_a_template_line_and_a_range() {
    let sfc = "\
<script setup lang=\"ts\">
import { ref } from 'vue';

const a = ref<number>(1);

function reset(): void {
  a.value = 0;
}
</script>

<template>
  <p>x</p>
</template>
";
    let fs = MemFs::with(&[("src/A.vue", sfc)]);
    let at = ReplaceAt::Method {
        class: None,
        name: "reset".to_owned(),
    };
    let out = go(
        &fs,
        "script>fn:reset():void{ a.value = 1 }",
        false,
        Target::Replace {
            file: "src/A.vue".to_owned(),
            at,
            expect: None,
        },
    )
    .unwrap();
    assert!(
        out.lines[0].starts_with("replaced src/A.vue L6-8 → L6-8 (reset)"),
        "{out:?}"
    );
    let file = fs.get("src/A.vue").unwrap();
    assert!(
        file.contains("function reset(): void {\n  a.value = 1\n}\n</script>"),
        "{file}"
    );

    go(
        &fs,
        "template>p{new}",
        false,
        Target::Replace {
            file: "src/A.vue".to_owned(),
            at: ReplaceAt::Lines(12, 12),
            expect: None,
        },
    )
    .unwrap();
    let file = fs.get("src/A.vue").unwrap();
    assert!(file.contains("  <p>new</p>\n</template>"), "{file}");
}

#[test]
fn invalid_script_is_rejected_and_nothing_is_written() {
    let fs = MemFs::default();
    let err = go(
        &fs,
        "vue:X>(script>fn:f(){ if ( })",
        false,
        write("src/X.vue"),
    )
    .unwrap_err();
    assert!(err.contains("syntax error in script at L"), "{err}");
    assert!(fs.get("src/X.vue").is_none());

    let err = go(
        &fs,
        "comp:useY>fn:g(){ x = = 1 }",
        false,
        write("src/useY.ts"),
    )
    .unwrap_err();
    assert!(err.contains("syntax error in src/useY.ts at L"), "{err}");
    assert!(fs.get("src/useY.ts").is_none());
}

#[test]
fn unbalanced_template_is_rejected() {
    let fs = MemFs::default();
    let err = go(&fs, "vue:X>(template>div{</b>})", false, write("src/X.vue")).unwrap_err();
    assert!(err.contains("syntax error in template at L"), "{err}");
    assert!(fs.get("src/X.vue").is_none());
}

#[test]
fn bad_targets_and_mixed_kinds_are_explained() {
    let fs = MemFs::default();
    let err = go(&fs, "comp:useA>ref:a:int=1", true, write("src/A.vue")).unwrap_err();
    assert!(err.contains("composable"), "{err}");
    let err = go(&fs, "vue:A>(template>p)", true, write("src/useA.ts")).unwrap_err();
    assert!(err.contains(".vue"), "{err}");
    let err = go(&fs, "comp:useA>props(a:str)", true, write("src/useA.ts")).unwrap_err();
    assert!(err.contains("props()"), "{err}");
    assert!(go(&fs, "script>ref:a:int=1", true, Target::Preview).is_ok());
}

#[test]
fn only_vue_js_ts_targets_and_heads_are_routed_here() {
    let req = |expr: &str, target: Target| GenRequest {
        expr: expr.to_owned(),
        dry: false,
        target,
    };
    assert!(!wants_vue(&req("c:A", write("app/A.php"))));
    assert!(!wants_vue(&req("c:A", Target::Preview)));
    assert!(!wants_vue(&req("+x()", into("app/A.php", None))));
    assert!(wants_vue(&req("vue:A>(template>p)", write("a/A.vue"))));
    assert!(wants_vue(&req("x", into("a/useA.ts", None))));
    assert!(wants_vue(&req("comp:useA>ref:a", Target::Preview)));
    assert!(wants_vue(&req("template>p", Target::Preview)));
}
