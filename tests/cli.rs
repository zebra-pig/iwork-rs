//! The binary, driven the way a shell drives it.
//!
//! The library's tests call methods. Somebody generating a document from a
//! script calls `iwork`, and the commands that give a document its look are
//! the ones a wrong argument is easiest to give — so these run the built
//! binary on documents made from nothing and read the result back.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use iwork::drawable::Fill;
use iwork::Document;

fn iwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_iwork"))
        .args(args)
        .output()
        .expect("the binary runs")
}

fn ok(args: &[&str]) -> String {
    let output = iwork(args);
    assert!(
        output.status.success(),
        "iwork {args:?} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iwork-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// `fill` and `text-look` on a spreadsheet made from nothing.
#[test]
fn cells_are_painted_and_set_from_the_shell() {
    let (a, b, c, d) = (
        scratch("a.numbers"),
        scratch("b.numbers"),
        scratch("c.numbers"),
        scratch("d.numbers"),
    );
    ok(&["create", "numbers", path(&a)]);
    let table = Document::open(&a).unwrap().tables()[0].name.clone();

    ok(&["set-cell", path(&a), &table, "A1", "Region", path(&b)]);
    let said = ok(&["fill", path(&b), &table, "A1:C1", "#122B4A", path(&c)]);
    assert!(said.contains("3 cell(s)"), "{said}");
    ok(&[
        "text-look",
        path(&c),
        &table,
        "A1:C1",
        "bold",
        "color=#FFFFFF",
        "size=13",
        "align=centre",
        path(&d),
    ]);

    let doc = Document::open(&d).unwrap();
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    let table = doc.table(&table).unwrap();
    assert!(table.audit().is_empty(), "{:?}", table.audit());
    for column in 0..3 {
        let record = &table.cell(0, column).expect("a record").record;
        assert!(record.cell_style_id.is_some(), "column {column} is painted");
        assert!(record.text_style_id.is_some(), "column {column} has a look");
    }
    // Empty cells were painted too, and the one with a value kept it.
    assert_eq!(table.value(0, 0).to_text(), "Region");

    let said = ok(&["check", path(&d)]);
    assert!(said.contains("no problems found"), "{said}");
}

/// `paint` and `background` on a deck made from nothing.
#[test]
fn a_shape_and_a_slide_are_painted_from_the_shell() {
    let (a, b, c, d) = (
        scratch("a.key"),
        scratch("b.key"),
        scratch("c.key"),
        scratch("d.key"),
    );
    ok(&["create", "keynote", path(&a)]);
    let slide = Document::open(&a).unwrap().slides()[0]
        .identifier
        .to_string();

    // By its place in the deck, the way `iwork slides` numbers it.
    ok(&["background", path(&a), "1", "0.07,0.17,0.29", path(&b)]);
    ok(&[
        "add-shape",
        path(&b),
        &slide,
        "ellipse",
        "Q3",
        "200",
        "200",
        "300",
        "300",
        path(&c),
    ]);
    let doc = Document::open(&c).unwrap();
    let shape = doc
        .drawables()
        .into_iter()
        .find(|d| d.text.is_some())
        .expect("the shape")
        .identifier
        .to_string();
    ok(&[
        "paint",
        path(&c),
        &shape,
        "fill=#B44A2B",
        "stroke=#FFFFFF:4",
        "opacity=0.9",
        path(&d),
    ]);

    let doc = Document::open(&d).unwrap();
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    let drawable = doc
        .drawables()
        .into_iter()
        .find(|d| d.identifier.to_string() == shape)
        .unwrap();
    let style = doc.object_style(drawable.style.unwrap()).unwrap();
    assert!(matches!(style.fill, Some(Fill::Color(c)) if (c.red - 180.0 / 255.0).abs() < 1e-4));
    assert_eq!(style.opacity, Some(0.9));
    assert_eq!(style.stroke.map(|s| s.width), Some(4.0));

    // The slide has a slide style of its own now, carrying the navy.
    let slide_style =
        iwork::style::reference_at(&doc.archive(doc.slides()[0].identifier).unwrap(), &[1, 1])
            .unwrap();
    match iwork::style::get_path(&doc.archive(slide_style).unwrap(), &[11, 1, 1, 5]) {
        Some(iwork::pb::Value::Fixed32(bytes)) => {
            assert!((f32::from_le_bytes(bytes) - 0.29).abs() < 1e-4)
        }
        other => panic!("the slide's background has no blue: {other:?}"),
    }
}

/// A wrong argument is an error with a sentence, a non-zero status, and no
/// output file.
#[test]
fn a_wrong_argument_writes_nothing() {
    let (a, out) = (scratch("wrong.numbers"), scratch("never.numbers"));
    ok(&["create", "numbers", path(&a)]);
    let table = Document::open(&a).unwrap().tables()[0].name.clone();

    for args in [
        vec![
            "fill",
            path(&a),
            table.as_str(),
            "A1",
            "chartreuse",
            path(&out),
        ],
        vec![
            "fill",
            path(&a),
            table.as_str(),
            "ZZ999",
            "#000000",
            path(&out),
        ],
        vec![
            "text-look",
            path(&a),
            table.as_str(),
            "A1",
            "wobbly",
            path(&out),
        ],
        vec![
            "text-look",
            path(&a),
            table.as_str(),
            "A1",
            "align=sideways",
            path(&out),
        ],
        vec!["paint", path(&a), "999999", "fill=#000000", path(&out)],
    ] {
        let output = iwork(&args);
        assert!(
            !output.status.success(),
            "iwork {args:?} should have failed"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).starts_with("error:"),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!out.exists(), "iwork {args:?} wrote a file anyway");
    }
}

/// Read through `head`, the binary ends the way a shell tool does — no panic
/// message, no backtrace hint.
#[test]
fn a_closed_pipe_is_not_a_panic() {
    use std::io::Read;
    use std::process::Stdio;
    let a = scratch("pipe.numbers");
    ok(&["create", "numbers", path(&a)]);
    // `dump` of the root object prints many lines; read one byte and hang up.
    let mut child = Command::new(env!("CARGO_BIN_EXE_iwork"))
        .args(["inspect", path(&a)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut one = [0u8; 1];
    stdout.read_exact(&mut one).unwrap();
    drop(stdout);
    let output = child.wait_with_output().unwrap();
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(!said.contains("panicked"), "{said}");
    assert!(!said.contains("RUST_BACKTRACE"), "{said}");
}
