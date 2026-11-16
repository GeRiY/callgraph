//! A HTML-generátor tesztjei (`html:` fej, `.html` célok, szerkesztés); a segédek a szülő `tests` modulból jönnek.

use super::*;

const PAGE: &str = "\
<!doctype html>
<html lang=\"en\">
<head>
  <title>T</title>
</head>
<body>
  <h1>Hi</h1>
</body>
</html>
";

const BOX: &str = "\
<body>
  <div id=\"box\">
    <p>a</p>
  </div>
  <div id=\"other\"></div>
</body>
";

fn into_at(file: &str, class: &str) -> Target {
    Target::Into {
        file: file.to_owned(),
        class: Some(class.to_owned()),
        place: Place::End,
        upsert: false,
    }
}

fn replace_lines(file: &str, from: usize, to: usize) -> Target {
    Target::Replace {
        file: file.to_owned(),
        at: ReplaceAt::Lines(from, to),
        expect: None,
    }
}

#[test]
fn full_document_with_head_shortcuts() {
    let fs = MemFs::default();
    let expr = "html:Home[lang=hu]>(head>meta:description=\"Hello world\"+meta:og:title=X+link:style.css+link:icon=/favicon.ico+link:preload=font.woff2[as=font crossorigin]+script:app.js+script:module=main.js+base:/app/+meta[name=robots content=noindex])+(body>h1{Hi})";
    let out = go(&fs, expr, true, write("index.html")).unwrap();
    let expected = "\
<!doctype html>
<html lang=\"hu\">
<head>
  <meta charset=\"utf-8\">
  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
  <title>Home</title>
  <meta name=\"description\" content=\"Hello world\">
  <meta property=\"og:title\" content=\"X\">
  <link rel=\"stylesheet\" href=\"style.css\">
  <link rel=\"icon\" href=\"/favicon.ico\">
  <link rel=\"preload\" href=\"font.woff2\" as=\"font\" crossorigin>
  <script src=\"app.js\" defer></script>
  <script type=\"module\" src=\"main.js\"></script>
  <base href=\"/app/\">
  <meta name=\"robots\" content=\"noindex\">
</head>
<body>
  <h1>Hi</h1>
</body>
</html>";
    assert_eq!(text(&out), expected);
    assert!(fs.get("index.html").is_none());

    // nem --dry: a fájl megíródik; a csupasz elemek dokumentumba kerülnek, a cím a fájlnév
    let out = go(&fs, "h1{Hi}+p.lead{Text}", false, write("about.html")).unwrap();
    assert!(text(&out).contains("wrote about.html"), "{out:?}");
    let written = fs.get("about.html").unwrap();
    assert!(
        written.starts_with("<!doctype html>\n<html lang=\"en\">"),
        "{written}"
    );
    assert!(written.contains("<title>about</title>"), "{written}");
    assert!(
        written.contains("<body>\n  <h1>Hi</h1>\n  <p class=\"lead\">Text</p>\n</body>"),
        "{written}"
    );
    let again = go(&fs, "p{x}", false, write("about.html")).unwrap_err();
    assert!(again.contains("already exists"), "{again}");

    // előnézet: `html:` fej cél nélkül
    let preview = go(&fs, "html:T>p{x}", true, Target::Preview).unwrap();
    assert!(text(&preview).contains("<title>T</title>"), "{preview:?}");
}

#[test]
fn form_with_select_options() {
    let fs = MemFs::default();
    let expr = "form[action=/x method=post]>label[for=e]{E-mail}+input[type=email name=e required]+select[name=s]>option*3{Opt $}^textarea[name=t]+fieldset>legend{L}^button[type=submit]{Go}";
    let out = go(&fs, expr, true, write("f.html")).unwrap();
    let expected = "\
  <form action=\"/x\" method=\"post\">
    <label for=\"e\">E-mail</label>
    <input type=\"email\" name=\"e\" required>
    <select name=\"s\">
      <option>Opt 1</option>
      <option>Opt 2</option>
      <option>Opt 3</option>
    </select>
    <textarea name=\"t\"></textarea>
    <fieldset>
      <legend>L</legend>
    </fieldset>
    <button type=\"submit\">Go</button>
  </form>";
    assert!(text(&out).contains(expected), "{}", text(&out));
}

#[test]
fn table_with_header_and_rows() {
    let fs = MemFs::default();
    let expr = "table>thead>tr>th*3{H $}^^tbody>tr*2>td*3{c}";
    let out = go(&fs, expr, true, write("t.html")).unwrap();
    let t = text(&out);
    assert!(
        t.contains(
            "      <tr>\n        <th>H 1</th>\n        <th>H 2</th>\n        <th>H 3</th>\n      </tr>"
        ),
        "{t}"
    );
    assert_eq!(t.matches("<tr>").count(), 3, "{t}");
    assert_eq!(t.matches("<td>c</td>").count(), 6, "{t}");
    assert!(t.contains("    </thead>\n    <tbody>"), "{t}");
}

#[test]
fn media_and_inline_svg() {
    let fs = MemFs::default();
    let expr = "picture>source[srcset=a.webp type=image/webp]+img[src=a.png alt=\"A cat\"]^video[controls]>source[src=v.mp4]^svg[viewBox=\"0 0 24 24\"]>path[d=\"M0 0h24v24H0z\"]+br";
    let out = go(&fs, expr, true, write("m.html")).unwrap();
    let expected = "\
  <picture>
    <source srcset=\"a.webp\" type=\"image/webp\">
    <img src=\"a.png\" alt=\"A cat\">
  </picture>
  <video controls>
    <source src=\"v.mp4\">
  </video>
  <svg viewBox=\"0 0 24 24\">
    <path d=\"M0 0h24v24H0z\" />
    <br>
  </svg>";
    assert!(text(&out).contains(expected), "{}", text(&out));
}

#[test]
fn boolean_data_aria_attributes_and_entities() {
    let fs = MemFs::default();
    let expr = "div#main.card.big[data-id=7 aria-label=\"Main area\" hidden class=x]>p{&copy; 2026 &amp; co}";
    let out = go(&fs, expr, true, write("a.html")).unwrap();
    let t = text(&out);
    assert!(
        t.contains(
            "<div class=\"card big x\" id=\"main\" data-id=\"7\" aria-label=\"Main area\" hidden>"
        ),
        "{t}"
    );
    assert!(t.contains("<p>&copy; 2026 &amp; co</p>"), "{t}");
}

#[test]
fn inline_style_script_and_comment() {
    let fs = MemFs::default();
    let expr = "style{ .a{color:red} }+script{ document.title = 'x' }+comment{ note }";
    let out = go(&fs, expr, true, write("i.html")).unwrap();
    let expected = "\
  <style>
    .a{color:red}
  </style>
  <script>
    document.title = 'x'
  </script>
  <!-- note -->";
    assert!(text(&out).contains(expected), "{}", text(&out));

    // CSS-szabályok `style>` alakban
    let out = go(&fs, "style>.a{color:red}", true, write("i.html")).unwrap();
    assert!(
        text(&out).contains("    .a {\n      color: red;\n    }"),
        "{}",
        text(&out)
    );

    // hibás CSS / JS / tag: nem íródik semmi
    let err = go(&fs, "style{ .a{color red} }", false, write("bad.html")).unwrap_err();
    assert!(err.contains("syntax error in style at L"), "{err}");
    let err = go(&fs, "script{ if ( }", false, write("bad.html")).unwrap_err();
    assert!(err.contains("syntax error in script at L"), "{err}");
    let err = go(&fs, "div{</b>}", false, write("bad.html")).unwrap_err();
    assert!(err.contains("syntax error in html at L"), "{err}");
    assert!(fs.get("bad.html").is_none());
}

/// A `--dry` `--into` / `--replace` célnál egy hely-fejléc sor áll a kód előtt.
#[test]
fn dry_into_and_replace_print_a_placement_header() {
    let fs = MemFs::with(&[("page.html", PAGE)]);
    let out = go(&fs, "p.note{Added}", true, into("page.html", None)).unwrap();
    assert!(out.lines[0].starts_with("→ page.html: html L"), "{out:?}");
    assert!(
        out.lines[1].contains("<p class=\"note\">Added</p>"),
        "{out:?}"
    );
    let out = go(&fs, "h2{New}", true, replace_lines("page.html", 7, 7)).unwrap();
    assert!(
        out.lines[0].starts_with("→ page.html replace L7-7 → L7-7"),
        "{out:?}"
    );
    assert_eq!(fs.get("page.html").unwrap(), PAGE);
}

#[test]
fn into_end_of_body_after_line_and_by_selector() {
    let fs = MemFs::with(&[("page.html", PAGE)]);
    let out = go(&fs, "p.note{Added}", false, into("page.html", None)).unwrap();
    assert!(text(&out).contains("inserted into page.html"), "{out:?}");
    let page = fs.get("page.html").unwrap();
    assert!(
        page.contains("  <h1>Hi</h1>\n  <p class=\"note\">Added</p>\n</body>"),
        "{page}"
    );
    assert!(fs.has_backup_of("page.html"));

    // --after L<n>: a nyitó sor után = az első gyerek
    let fs = MemFs::with(&[("page.html", PAGE)]);
    go(&fs, "nav{Menu}", false, into("page.html", Some("L6"))).unwrap();
    let page = fs.get("page.html").unwrap();
    assert!(
        page.contains("<body>\n  <nav>Menu</nav>\n  <h1>Hi</h1>"),
        "{page}"
    );

    // ::#id = az elem utolsó gyereke; az üres egysoros elem szétvágva
    let fs = MemFs::with(&[("box.html", BOX)]);
    go(&fs, "span{new}", false, into_at("box.html", "#box")).unwrap();
    go(&fs, "b{x}", false, into_at("box.html", "#other")).unwrap();
    let b = fs.get("box.html").unwrap();
    assert!(
        b.contains("    <p>a</p>\n    <span>new</span>\n  </div>"),
        "{b}"
    );
    assert!(
        b.contains("  <div id=\"other\">\n    <b>x</b>\n  </div>\n</body>"),
        "{b}"
    );

    // ::tag és ::.class
    let fs = MemFs::with(&[(
        "c.html",
        "<body>\n<ul class=\"nav\">\n<li>a</li>\n</ul>\n</body>\n",
    )]);
    go(&fs, "li{b}", false, into_at("c.html", ".nav")).unwrap();
    go(&fs, "li{c}", false, into_at("c.html", "ul")).unwrap();
    let c = fs.get("c.html").unwrap();
    assert!(
        c.contains("<ul class=\"nav\">\n<li>a</li>\n<li>b</li>\n<li>c</li>\n</ul>"),
        "{c}"
    );

    let err = go(&fs, "p{x}", false, into_at("c.html", "#nope")).unwrap_err();
    assert!(err.contains("no element matches"), "{err}");
}

#[test]
fn replace_by_lines_and_by_id_and_duplicate_warning() {
    let fs = MemFs::with(&[("page.html", PAGE), ("box.html", BOX)]);
    let out = go(&fs, "h2{New}", false, replace_lines("page.html", 7, 7)).unwrap();
    assert!(text(&out).contains("replaced page.html L7-7"), "{out:?}");
    assert!(
        fs.get("page.html")
            .unwrap()
            .contains("  <h2>New</h2>\n</body>")
    );
    assert!(fs.has_backup_of("page.html"));

    let target = Target::Replace {
        file: "box.html".to_owned(),
        at: ReplaceAt::Method {
            class: None,
            name: "#box".to_owned(),
        },
        expect: None,
    };
    go(&fs, "section#box>p{b}", false, target).unwrap();
    let b = fs.get("box.html").unwrap();
    assert!(
        b.contains(
            "<body>\n  <section id=\"box\">\n    <p>b</p>\n  </section>\n  <div id=\"other\"></div>"
        ),
        "{b}"
    );

    // duplikált id: figyelmeztetés, nem hiba
    let out = go(&fs, "div#other", false, into("box.html", None)).unwrap();
    assert!(text(&out).contains("duplicate id `other`"), "{out:?}");
    assert_eq!(
        fs.get("box.html").unwrap().matches("id=\"other\"").count(),
        2
    );
}

#[test]
fn unbalanced_existing_file_and_dispatch() {
    let fs = MemFs::with(&[("bad.html", "<body>\n<div>\n</body>\n")]);
    let err = go(&fs, "p{x}", false, into("bad.html", None)).unwrap_err();
    assert!(err.contains("syntax error in bad.html at L"), "{err}");
    let err = go(&fs, "html:T>p{x}", false, into("bad.html", None)).unwrap_err();
    assert!(err.contains("--write"), "{err}");

    let req = |expr: &str, target: Target| GenRequest {
        expr: expr.to_owned(),
        mode: Mode::Write,
        target,
    };
    assert!(wants_vue(&req("html:T>p", Target::Preview)));
    assert!(wants_vue(&req("p", write("a.htm"))));
    assert!(wants_vue(&req("p", into("a.html", None))));
    assert!(!wants_vue(&req("div", Target::Preview)));
}
