//! A Vue-generátor egységtesztjei, memóriabeli fájlrendszerrel és rögzített órával (mint a PHP-teszteknél).

use std::cell::Cell;
use std::path::Path;

use super::{VueGenerator, wants_vue};
use crate::generate::fs::Env;
use crate::generate::generator::{GenRequest, Mode, Output, Place, ReplaceAt, Target};
use crate::generate::testing::{FixedClock, MemFs, ROOT};

const T0: u64 = 1_700_000_000;

fn go(fs: &MemFs, expr: &str, dry: bool, target: Target) -> Result<Output, String> {
    go_with(fs, expr, Mode::of(dry, false), target)
}

fn go_with(fs: &MemFs, expr: &str, mode: Mode, target: Target) -> Result<Output, String> {
    let clock = FixedClock(Cell::new(T0));
    let env = Env {
        root: Path::new(ROOT),
        fs,
        clock: &clock,
    };
    let request = GenRequest {
        expr: expr.to_owned(),
        mode,
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
        place: place_after(after),
        upsert: false,
    }
}

/// A `--after` neve (vagy semmi) `Place`-ként.
fn place_after(name: Option<&str>) -> Place {
    name.map_or(Place::End, |m| Place::After(m.to_owned()))
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
fn expect_guards_vue_and_html_replaces() {
    let fs = MemFs::with(&[("src/A.vue", EXISTING_SFC)]);
    let target = |file: &str, at: ReplaceAt| Target::Replace {
        file: file.to_owned(),
        at,
        expect: Some("deadbeef".to_owned()),
    };
    let err = go(
        &fs,
        "template>p{y}",
        false,
        target("src/A.vue", ReplaceAt::Lines(9, 9)),
    )
    .unwrap_err();
    assert!(err.contains("changed"), "{err}");
    assert_eq!(fs.get("src/A.vue").unwrap(), EXISTING_SFC);

    let html = "<div id=\"a\">\n  <p>x</p>\n</div>\n";
    let fs = MemFs::with(&[("page.html", html)]);
    let err = go(
        &fs,
        "p{y}",
        false,
        target(
            "page.html",
            ReplaceAt::Method {
                class: None,
                name: "#a".to_owned(),
            },
        ),
    )
    .unwrap_err();
    assert!(err.contains("changed"), "{err}");
    assert_eq!(fs.get("page.html").unwrap(), html);
}

#[test]
fn crlf_vue_files_keep_their_line_endings() {
    let crlf = EXISTING_SFC.replace('\n', "\r\n");
    let fs = MemFs::with(&[("src/A.vue", crlf.as_str())]);
    go(&fs, "template>span{y}", false, into("src/A.vue", None)).unwrap();
    let file = fs.get("src/A.vue").unwrap();
    assert!(file.contains("<span>y</span>"), "{file:?}");
    assert!(!file.replace("\r\n", "").contains('\n'), "{file:?}");
}

const NESTED_SFC: &str = "\
<script setup lang=\"ts\">
import { ref } from 'vue';
</script>

<template>
  <div>
    <ul id=\"list\">
      <li>a</li>
    </ul>
    <p>x</p>
  </div>
</template>
";

#[test]
fn into_template_element_by_id_appends_as_its_last_child() {
    let fs = MemFs::with(&[("src/A.vue", NESTED_SFC)]);
    let target = |class: &str| Target::Into {
        file: "src/A.vue".to_owned(),
        class: Some(class.to_owned()),
        place: Place::End,
        upsert: false,
    };
    go(&fs, "template>li{b}", false, target("template#list")).unwrap();
    let file = fs.get("src/A.vue").unwrap();
    assert!(
        file.contains("      <li>a</li>\n      <li>b</li>\n    </ul>\n    <p>x</p>"),
        "{file}"
    );
    // a `template` előtag nélkül is megy, és a hiányzó elem hiba
    go(&fs, "template>li{c}", false, target("#list")).unwrap();
    assert!(
        fs.get("src/A.vue")
            .unwrap()
            .contains("<li>c</li>\n    </ul>")
    );
    let err = go(&fs, "template>li{d}", false, target("template#nope")).unwrap_err();
    assert!(err.contains("no element matches"), "{err}");
}

#[test]
fn upsert_and_diff_work_on_class_members() {
    let fs = MemFs::with(&[("src/svc.ts", SERVICE_TS)]);
    let upsert = Target::Into {
        file: "src/svc.ts".to_owned(),
        class: Some("Svc".to_owned()),
        place: Place::End,
        upsert: true,
    };
    let out = go_with(&fs, "+run():void{ this.n = 5 }", Mode::Write, upsert).unwrap();
    assert!(out.lines[0].starts_with("replaced src/svc.ts"), "{out:?}");
    assert!(fs.get("src/svc.ts").unwrap().contains("this.n = 5"));

    let fs = MemFs::with(&[("src/svc.ts", SERVICE_TS)]);
    let out = go_with(
        &fs,
        "+run():void{ this.n = 5 }",
        Mode::Diff,
        Target::Replace {
            file: "src/svc.ts".to_owned(),
            at: ReplaceAt::Method {
                class: Some("Svc".to_owned()),
                name: "run".to_owned(),
            },
            expect: None,
        },
    )
    .unwrap();
    assert!(
        out.lines
            .iter()
            .any(|l| l.starts_with("- ") && l.contains("this.n++")),
        "{out:?}"
    );
    assert!(
        out.lines
            .iter()
            .any(|l| l.starts_with("+ ") && l.contains("this.n = 5")),
        "{out:?}"
    );
    assert_eq!(fs.get("src/svc.ts").unwrap(), SERVICE_TS);
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

/// A `--dry` `--into` / `--replace` célnál egy hely-fejléc sor áll a kód előtt (a fájl nem változik).
#[test]
fn dry_into_and_replace_print_a_placement_header() {
    let fs = MemFs::with(&[("src/A.vue", EXISTING_SFC)]);
    let out = go(
        &fs,
        "script>fn:reset():void{ a.value = 0 }",
        true,
        into("src/A.vue", None),
    )
    .unwrap();
    assert!(out.lines[0].starts_with("→ src/A.vue: script L"), "{out:?}");
    assert!(out.lines[1..].iter().any(|l| l.contains("function reset")));
    assert_eq!(fs.get("src/A.vue").unwrap(), EXISTING_SFC);

    let out = go(
        &fs,
        "script>ref:a:int=2",
        true,
        Target::Replace {
            file: "src/A.vue".to_owned(),
            at: ReplaceAt::Lines(4, 4),
            expect: None,
        },
    )
    .unwrap();
    assert!(
        out.lines[0].starts_with("→ src/A.vue replace L4-4 → L"),
        "{out:?}"
    );
    assert_eq!(fs.get("src/A.vue").unwrap(), EXISTING_SFC);
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
        mode: Mode::Write,
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

// --- composable-bekötés: nevesített import, auto-felismerés ---

#[test]
fn named_use_imports_with_braces() {
    let fs = MemFs::default();
    let out = go(
        &fs,
        "vue:U>(script>use:{useCart}@/composables/useCart+use:{a,b as c}@/x+use:vue(ref))",
        true,
        write("src/U.vue"),
    )
    .unwrap();
    let expected = "\
<script setup>
import { ref } from 'vue';
import { useCart } from '@/composables/useCart';
import { a, b as c } from '@/x';
</script>";
    assert_eq!(text(&out), expected);
}

#[test]
fn bare_use_path_detects_named_export_versus_default() {
    let fs = MemFs::with(&[
        (
            "resources/js/composables/useCart.ts",
            "export function useCart() {}\n",
        ),
        (
            "resources/js/composables/useLoad.js",
            "export async function useLoad() {}\n",
        ),
        (
            "resources/js/composables/useArrow.js",
            "export const useArrow = () => {}\n",
        ),
        (
            "resources/js/composables/useOld.ts",
            "export default function useOld() {}\nexport function useOld2() {}\n",
        ),
        (
            "resources/js/composables/useIdx/index.ts",
            "export function useIdx() {}\n",
        ),
        (
            "resources/js/Components/Card.vue",
            "<template><div/></template>\n",
        ),
    ]);
    let out = go(
        &fs,
        "vue:P>(script>use:@/composables/useCart+use:@/composables/useLoad+use:@/composables/useArrow+use:@/composables/useOld+use:@/composables/useIdx+use:@/composables/useMissing+use:@/Components/Card)",
        true,
        write("resources/js/Pages/P.vue"),
    )
    .unwrap();
    let code = text(&out);
    for line in [
        "import Card from '@/Components/Card.vue';",
        "import { useArrow } from '@/composables/useArrow';",
        "import { useCart } from '@/composables/useCart';",
        "import { useIdx } from '@/composables/useIdx';",
        "import { useLoad } from '@/composables/useLoad';",
        "import useMissing from '@/composables/useMissing';",
        "import useOld from '@/composables/useOld';",
    ] {
        assert!(code.contains(line), "{line}\n{code}");
    }
}

#[test]
fn use_hint_suggests_the_brace_form() {
    let fs = MemFs::default();
    let err = go(
        &fs,
        "vue:U>(script>use:@/x/use-cart)",
        true,
        write("src/U.vue"),
    )
    .unwrap_err();
    assert!(err.contains("{Name}@/x/use-cart"), "{err}");
    assert!(!err.contains("::"), "{err}");
    let err = go(
        &fs,
        "vue:U>(script>use:@/c/useX::useX as useX)",
        true,
        write("src/U.vue"),
    )
    .unwrap_err();
    assert!(err.contains("{useX}@/composables/useX"), "{err}");
}

#[test]
fn named_use_merges_into_an_existing_import_line() {
    let existing = "<script setup>\nimport { ref } from 'vue';\nimport { useCart } from '@/composables/useCart';\n\nconst a = ref(1);\n</script>\n\n<template>\n  <div />\n</template>\n";
    let fs = MemFs::with(&[("src/M.vue", existing)]);
    go(
        &fs,
        "script>use:{useCart,useB}@/composables/useCart+ref:q:int=1",
        false,
        into("src/M.vue", None),
    )
    .unwrap();
    let file = fs.get("src/M.vue").unwrap();
    assert!(
        file.contains("import { useCart, useB } from '@/composables/useCart';"),
        "{file}"
    );
    assert_eq!(file.matches("import ").count(), 2, "{file}");
}

// --- inline objektum-típus, `type:` tag ---

#[test]
fn inline_object_types_in_props_and_refs() {
    let fs = MemFs::with(TS_PROJECT);
    let expr = "vue:L>(script>props(items:list<{ flag: str; on?: bool }>,pair:{ a: int, b: list<str> })+ref:cur:opt<{ flag: str }>)";
    let out = go(&fs, expr, true, write("src/L.vue")).unwrap();
    let code = text(&out);
    assert!(
        code.contains("  items: { flag: string; on?: boolean }[];"),
        "{code}"
    );
    assert!(
        code.contains("  pair: { a: number; b: string[] };"),
        "{code}"
    );
    assert!(
        code.contains("const cur = ref<{ flag: string } | null>(null);"),
        "{code}"
    );
    let js = go(&MemFs::default(), expr, true, write("src/L.vue")).unwrap();
    let code = text(&js);
    assert!(
        code.contains("items: { type: Array, required: true },"),
        "{code}"
    );
    assert!(
        code.contains("pair: { type: Object, required: true },"),
        "{code}"
    );
    assert!(code.contains("const cur = ref(null);"), "{code}");
}

#[test]
fn type_member_in_a_composable_ts_and_js() {
    let fs = MemFs::with(TS_PROJECT);
    let expr = "comp:useFlags>type:Flag{flag:str,on?:bool,tags:list<str>}+ref:flags:list<Flag>=[]";
    let out = go(&fs, expr, true, write("src/composables/useFlags.ts")).unwrap();
    let expected = "\
import { ref } from 'vue';

export interface Flag { flag: string; on?: boolean; tags: string[] }

export function useFlags() {
  const flags = ref<Flag[]>([]);

  return {
    flags,
  };
}";
    assert_eq!(text(&out), expected);

    let js = go(&fs, expr, true, write("src/composables/useFlags.js")).unwrap();
    let code = text(&js);
    assert!(
        code.contains("/** @typedef {{ flag: string, on?: boolean, tags: string[] }} Flag */"),
        "{code}"
    );
    assert!(code.contains("const flags = ref([]);"), "{code}");
    assert!(!code.contains("interface"), "{code}");
    assert!(!code.contains("    Flag,"), "{code}");
}

#[test]
fn type_member_in_an_sfc_is_a_plain_interface() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(
        &fs,
        "vue:T>(script>type:Flag{flag:str}+props(items:list<Flag>))",
        true,
        write("src/T.vue"),
    )
    .unwrap();
    let code = text(&out);
    assert!(
        code.contains("\ninterface Flag { flag: string }\n"),
        "{code}"
    );
    assert!(!code.contains("export interface"), "{code}");
    assert!(code.contains("  items: Flag[];"), "{code}");
}

// --- szöveg-interpoláció ---

#[test]
fn text_blocks_starting_with_a_mustache() {
    let fs = MemFs::default();
    let out = go(
        &fs,
        "vue:T>(template>span{{{ x }}}+p{{{ a }} / {{ b }}}+span{{ y }}+span{Text {{ a }}}+button{Save})",
        true,
        write("src/T.vue"),
    )
    .unwrap();
    let code = text(&out);
    for line in [
        "  <span>{{ x }}</span>",
        "  <p>{{ a }} / {{ b }}</p>",
        "  <span>{{ y }}</span>",
        "  <span>Text {{ a }}</span>",
        "  <button>Save</button>",
    ] {
        assert!(code.contains(line), "{line}\n{code}");
    }
    assert!(!code.contains("{{{"), "{code}");
}

// --- raw{} és const:{}=… ---

#[test]
fn raw_and_destructuring_in_an_sfc() {
    let fs = MemFs::default();
    let out = go(
        &fs,
        "vue:R>(script>const:{ a, b: c }=useX(items)+raw{ const { d } = other() }+ref:n:int=0)",
        true,
        write("src/R.vue"),
    )
    .unwrap();
    let code = text(&out);
    assert!(code.contains("const { a, b: c } = useX(items);"), "{code}");
    assert!(code.contains("\nconst { d } = other()\n"), "{code}");
    let first = code.find("useX(items)").unwrap();
    let second = code.find("other()").unwrap();
    let third = code.find("ref(0)").unwrap();
    assert!(first < second && second < third, "{code}");
    // pontosvessző nélküli projekt: a destructuring sem kap
    let plain = MemFs::with(&[(
        "resources/js/A.vue",
        "<script setup>\nconst x = 1\n</script>\n",
    )]);
    let out = go(&plain, "script>const:{ a }=useX()", true, Target::Preview).unwrap();
    assert_eq!(text(&out), "const { a } = useX()");
}

#[test]
fn raw_and_destructuring_in_a_composable_are_not_returned() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(
        &fs,
        "comp:useBox>const:{ items, total }=useCart()+ref:n:int=0+raw{ console.log(n.value) }",
        true,
        write("src/composables/useBox.ts"),
    )
    .unwrap();
    let expected = "\
import { ref } from 'vue';

export function useBox() {
  const { items, total } = useCart();

  const n = ref<number>(0);

  console.log(n.value)

  return {
    n,
  };
}";
    assert_eq!(text(&out), expected);
    let js = go(
        &MemFs::default(),
        "comp:useBox>const:{ items }=useCart()",
        true,
        write("src/composables/useBox.js"),
    )
    .unwrap();
    let code = text(&js);
    assert!(code.contains("  const { items } = useCart();"), "{code}");
}

#[test]
fn destructured_names_count_as_declared_for_into() {
    let existing = "<script setup>\nconst { a, b: c } = useX();\n</script>\n\n<template>\n  <div />\n</template>\n";
    let fs = MemFs::with(&[("src/D.vue", existing)]);
    for expr in ["script>ref:a:int=1", "script>fn:c(){}"] {
        let err = go(&fs, expr, false, into("src/D.vue", None)).unwrap_err();
        assert!(err.contains("already declared"), "{err}");
    }
    let err = go(
        &fs,
        "script>const:{ q, a }=useY()",
        false,
        into("src/D.vue", None),
    )
    .unwrap_err();
    assert!(err.contains("already declared"), "{err}");
}

// --- általános JS / TS modulok: mod:, class:, export-módosítók ---

fn into_class(file: &str, class: &str, after: Option<&str>) -> Target {
    Target::Into {
        file: file.to_owned(),
        class: Some(class.to_owned()),
        place: place_after(after),
        upsert: false,
    }
}

fn replace_at(file: &str, class: Option<&str>, name: &str) -> Target {
    Target::Replace {
        file: file.to_owned(),
        at: ReplaceAt::Method {
            class: class.map(str::to_owned),
            name: name.to_owned(),
        },
        expect: None,
    }
}

const MODULE_EXPR: &str = "mod:strings>type:Opts{sep?:str,max:int}+fn:join(parts:list<str>,o?:Opts):str{ return parts.join(o?.sep ?? ',') }+const:LIMIT:int=10+-fn:clean(s:str):str{ return s.trim() }";

#[test]
fn module_in_typescript_with_type_function_const_and_private_helper() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(&fs, MODULE_EXPR, true, write("src/strings.ts")).unwrap();
    let expected = "\
export interface Opts { sep?: string; max: number }

export function join(parts: string[], o?: Opts): string {
  return parts.join(o?.sep ?? ',')
}

export const LIMIT: number = 10;

function clean(s: string): string {
  return s.trim()
}";
    assert_eq!(text(&out), expected);
}

#[test]
fn module_in_javascript_gets_jsdoc_and_no_types() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(&fs, MODULE_EXPR, true, write("src/strings.js")).unwrap();
    let code = text(&out);
    assert!(
        code.starts_with("/** @typedef {{ sep?: string, max: number }} Opts */\n\n/**\n"),
        "{code}"
    );
    assert!(
        code.contains(
            " * @param {string[]} parts\n * @param {Opts} [o]\n * @returns {string}\n */\nexport function join(parts, o) {"
        ),
        "{code}"
    );
    assert!(code.contains("export const LIMIT = 10;"), "{code}");
    assert!(code.contains("\nfunction clean(s) {"), "{code}");
    assert!(!code.contains("export function clean"), "{code}");
    // a fájlba is megy, és a kiterjesztés a nyelv
    let written = go(&fs, MODULE_EXPR, false, write("src/strings2.mjs")).unwrap();
    assert!(written.lines[0].starts_with("wrote src/strings2.mjs"));
}

#[test]
fn bare_members_to_a_ts_target_are_a_module_but_ref_stays_a_composable() {
    let fs = MemFs::with(TS_PROJECT);
    let out = go(
        &fs,
        "fn:add(a:int,b:int):int{ return a + b }",
        true,
        write("src/math.ts"),
    )
    .unwrap();
    assert!(
        text(&out).starts_with("export function add(a: number, b: number): number {"),
        "{out:?}"
    );
    let out = go(&fs, "ref:n:int=0", true, write("src/composables/useN.ts")).unwrap();
    assert!(text(&out).contains("export function useN() {"), "{out:?}");
}

#[test]
fn type_alias_default_export_and_private_const() {
    let fs = MemFs::default();
    let expr =
        "mod:m>type:Id=str|int+type:Mode='a'|'b'+-const:SECRET:str='x'+!fn:main():void{ run() }";
    let out = go(&fs, expr, true, write("src/m.ts")).unwrap();
    let code = text(&out);
    assert!(code.contains("export type Id = string | number;"), "{code}");
    assert!(code.contains("export type Mode = 'a' | 'b';"), "{code}");
    assert!(code.contains("\nconst SECRET: string = 'x';"), "{code}");
    assert!(
        code.contains("export default function main(): void {\n  run()\n}"),
        "{code}"
    );
    let js = text(&go(&fs, expr, true, write("src/m.js")).unwrap());
    assert!(js.contains("/** @typedef {string | number} Id */"), "{js}");
    assert!(js.contains("/** @typedef {'a' | 'b'} Mode */"), "{js}");
    assert!(js.contains("export default function main() {"), "{js}");

    let out = go(&fs, "!const:X:int=1", true, write("src/x.ts")).unwrap();
    assert_eq!(text(&out), "const X: number = 1;\nexport default X;");
    let err = go(&fs, "!type:Id=str|int", true, write("src/x.ts")).unwrap_err();
    assert!(err.contains("export default"), "{err}");
}

#[test]
fn class_in_typescript_has_parameter_properties_fields_and_modifiers() {
    let fs = MemFs::default();
    let expr = "class:UserService<BaseService>[Disposable,Loggable]>$count:int=0+-$secret:str+%$id:str+ctor(-repo:Repo,+retries:int=3)+ +load(id:int):User{ return this.repo.find(id) }+-::make(r:Repo):UserService{ return new UserService(r) }+#touch():void{ this.count++ }+ +async:fetch():list<User>{ return [] }";
    let out = go(&fs, expr, true, write("src/UserService.ts")).unwrap();
    let expected = "\
export class UserService extends BaseService implements Disposable, Loggable {
  count: number = 0;
  private secret: string;
  readonly id: string;

  constructor(private readonly repo: Repo, public retries: number = 3) {}

  load(id: number): User {
    return this.repo.find(id)
  }

  private static make(r: Repo): UserService {
    return new UserService(r)
  }

  protected touch(): void {
    this.count++
  }

  async fetch(): Promise<User[]> {
    return []
  }
}";
    assert_eq!(text(&out), expected);

    let abs = go(
        &fs,
        "class~:Shape>~area():float+ +name():str{ return 'x' }",
        true,
        write("src/Shape.ts"),
    )
    .unwrap();
    let code = text(&abs);
    assert!(
        code.starts_with(
            "export abstract class Shape {\n  abstract area(): number;\n\n  name(): string {"
        ),
        "{code}"
    );
}

#[test]
fn class_in_javascript_assigns_constructor_params_and_uses_jsdoc() {
    let fs = MemFs::default();
    let expr = "class:Counter>$count:int=0+ctor(-step:int=1)+ +inc():void{ this.count += this.step }+::create():Counter{ return new Counter() }+-hidden():int{ return 1 }";
    let out = go(&fs, expr, true, write("src/Counter.js")).unwrap();
    let code = text(&out);
    assert!(
        code.starts_with("export class Counter {\n  /** @type {number} */\n  count = 0;"),
        "{code}"
    );
    assert!(
        code.contains("   * @param {number} [step=1]\n   */\n  constructor(step = 1) {\n    this.step = step;\n  }"),
        "{code}"
    );
    assert!(
        code.contains("\n  inc() {\n    this.count += this.step\n  }"),
        "{code}"
    );
    assert!(
        code.contains("   * @returns {Counter}\n   */\n  static create() {"),
        "{code}"
    );
    // JS-ben nincs láthatósági módosító
    assert!(code.contains("\n  hidden() {"), "{code}");
    assert!(!code.contains("private"), "{code}");
}

const SERVICE_TS: &str = "\
export class Svc {
  private n = 0;

  run(): void {
    this.n++;
  }

  stop(): void {
    this.n = 0;
  }
}
";

#[test]
fn into_class_inserts_members_after_a_method_or_at_the_end() {
    let fs = MemFs::with(&[("src/svc.ts", SERVICE_TS)]);
    let out = go(
        &fs,
        "+pause():void{ this.n = -1 }",
        false,
        into_class("src/svc.ts", "Svc", Some("run")),
    )
    .unwrap();
    assert!(
        out.lines[0].starts_with("inserted into src/svc.ts"),
        "{out:?}"
    );
    let file = fs.get("src/svc.ts").unwrap();
    assert!(
        file.contains("  run(): void {\n    this.n++;\n  }\n\n  pause(): void {\n    this.n = -1\n  }\n\n  stop(): void {"),
        "{file}"
    );
    assert!(fs.has_backup_of("src/svc.ts"));

    go(
        &fs,
        "+extra():int{ return 1 }",
        false,
        into_class("src/svc.ts", "Svc", None),
    )
    .unwrap();
    let file = fs.get("src/svc.ts").unwrap();
    assert!(
        file.ends_with("    this.n = 0;\n  }\n\n  extra(): number {\n    return 1\n  }\n}\n"),
        "{file}"
    );

    let err = go(
        &fs,
        "+run():void{}",
        false,
        into_class("src/svc.ts", "Svc", None),
    )
    .unwrap_err();
    assert!(err.contains("already a member"), "{err}");
    let err = go(
        &fs,
        "+x():void{}",
        false,
        into_class("src/svc.ts", "Nope", None),
    )
    .unwrap_err();
    assert!(err.contains("no class `Nope`"), "{err}");
    let err = go(
        &fs,
        "+x():void{}",
        false,
        into_class("src/svc.ts", "Svc", Some("zzz")),
    )
    .unwrap_err();
    assert!(err.contains("no `zzz`"), "{err}");
}

const UTIL_TS: &str = "\
import { ref } from 'vue';

export function one(): number {
  return 1;
}

export function three(): number {
  return 3;
}
";

#[test]
fn into_module_appends_merges_imports_and_refuses_duplicates() {
    let fs = MemFs::with(&[("src/util.ts", UTIL_TS)]);
    go(
        &fs,
        "use:vue(ref,computed)+fn:two():int{ return 2 }",
        false,
        into("src/util.ts", None),
    )
    .unwrap();
    let file = fs.get("src/util.ts").unwrap();
    assert!(
        file.starts_with("import { ref, computed } from 'vue';\n"),
        "{file}"
    );
    assert!(
        file.ends_with("export function three(): number {\n  return 3;\n}\n\nexport function two(): number {\n  return 2\n}\n"),
        "{file}"
    );
    assert!(fs.has_backup_of("src/util.ts"));

    let err = go(
        &fs,
        "fn:one():int{ return 9 }",
        false,
        into("src/util.ts", None),
    )
    .unwrap_err();
    assert!(err.contains("already declared"), "{err}");
    let err = go(&fs, "type:three=int", false, into("src/util.ts", None)).unwrap_err();
    assert!(err.contains("already declared"), "{err}");

    go(
        &fs,
        "const:MID:int=5",
        false,
        into("src/util.ts", Some("one")),
    )
    .unwrap();
    let file = fs.get("src/util.ts").unwrap();
    let (one, mid, three) = (
        file.find("function one").unwrap(),
        file.find("export const MID: number = 5;").unwrap(),
        file.find("function three").unwrap(),
    );
    assert!(one < mid && mid < three, "{file}");
}

#[test]
fn replace_a_module_function_a_private_helper_and_a_class_method() {
    let util = "\
export function run(a: number): number {
  return a;
}

function helper(): number {
  return 1;
}
";
    let fs = MemFs::with(&[("src/u.ts", util), ("src/svc.ts", SERVICE_TS)]);
    let out = go(
        &fs,
        "fn:run(a:int):int{ return a * 2 }",
        false,
        replace_at("src/u.ts", None, "run"),
    )
    .unwrap();
    assert!(
        out.lines[0].starts_with("replaced src/u.ts L1-3"),
        "{out:?}"
    );
    let file = fs.get("src/u.ts").unwrap();
    assert!(
        file.starts_with(
            "export function run(a: number): number {\n  return a * 2\n}\n\nfunction helper()"
        ),
        "{file}"
    );

    // a nem exportált marad nem exportált
    go(
        &fs,
        "fn:helper():int{ return 2 }",
        false,
        replace_at("src/u.ts", None, "helper"),
    )
    .unwrap();
    let file = fs.get("src/u.ts").unwrap();
    assert!(
        file.contains("\nfunction helper(): number {\n  return 2\n}\n"),
        "{file}"
    );

    let out = go(
        &fs,
        "+run():void{ this.n += 2 }",
        false,
        replace_at("src/svc.ts", Some("Svc"), "run"),
    )
    .unwrap();
    assert!(out.lines[0].contains("Svc::run"), "{out:?}");
    let file = fs.get("src/svc.ts").unwrap();
    assert!(
        file.contains("  run(): void {\n    this.n += 2\n  }\n\n  stop(): void {"),
        "{file}"
    );
    assert!(!file.contains("this.n++"), "{file}");

    let err = go(
        &fs,
        "+nope():void{}",
        false,
        replace_at("src/svc.ts", Some("Svc"), "nope"),
    )
    .unwrap_err();
    assert!(err.contains("no `nope`"), "{err}");
}

#[test]
fn replace_a_whole_class_by_name() {
    let fs = MemFs::with(&[("src/svc.ts", SERVICE_TS)]);
    go(
        &fs,
        "class:Svc>+ping():void{ }",
        false,
        replace_at("src/svc.ts", None, "Svc"),
    )
    .unwrap();
    let file = fs.get("src/svc.ts").unwrap();
    assert!(
        file.starts_with("export class Svc {\n  ping(): void {"),
        "{file}"
    );
    assert!(!file.contains("stop"), "{file}");
}

#[test]
fn module_errors_and_routing() {
    let fs = MemFs::default();
    let err = go(&fs, "mod:x>fn:f(){ if ( }", false, write("src/x.ts")).unwrap_err();
    assert!(err.contains("syntax error in src/x.ts at L"), "{err}");
    assert!(fs.get("src/x.ts").is_none());
    let err = go(&fs, "mod:x>ref:a:int=1", true, write("src/x.ts")).unwrap_err();
    assert!(err.contains("not to a module"), "{err}");
    let err = go(&fs, "mod:x>fn:f(){}", true, write("src/X.vue")).unwrap_err();
    assert!(err.contains("`mod:`"), "{err}");
    let err = go(
        &fs,
        "comp:useA>class:A>+x():int{ return 1 }",
        true,
        write("src/useA.ts"),
    )
    .unwrap_err();
    assert!(err.contains("belong to a module"), "{err}");

    let req = |expr: &str, target: Target| GenRequest {
        expr: expr.to_owned(),
        mode: Mode::Write,
        target,
    };
    assert!(wants_vue(&req("mod:x>fn:a", Target::Preview)));
    assert!(wants_vue(&req("class:A>+x():int", Target::Preview)));
    assert!(wants_vue(&req("class~:A", Target::Preview)));
    assert!(!wants_vue(&req("c:A", Target::Preview)));
    let preview = go(&fs, "mod:x>fn:f():int{ return 1 }", true, Target::Preview).unwrap();
    assert!(
        text(&preview).contains("export function f()"),
        "{preview:?}"
    );
}

mod html;
