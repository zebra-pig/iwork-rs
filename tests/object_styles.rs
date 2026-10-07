//! Painting a drawable: fill, outline and opacity.
//!
//! Two things had to be measured before any of this could be claimed, and
//! both were measured the same way — by making the app open the document and
//! **save it again**, then reading what it wrote.
//!
//! **A drawable needs a style of its own.** A document from nothing points
//! every shape at a theme preset — `line-style-preset-0` and its siblings —
//! and painting a preset does not survive: Keynote regenerated its presets on
//! save and the colour was gone. What the app does instead is give the object
//! its own archive naming the preset as parent and carrying only what differs.
//! That is what the first paint now makes.
//!
//! **`override_count` is not decoration.** A variation whose bag held a
//! colour while its count said `0` came back from Keynote's save with the
//! properties stripped out. The count is maintained from the bag, so the two
//! cannot disagree.
//!
//! With both in place the colours survive: a red rectangle and a blue ellipse
//! written here read back out of the file Keynote itself wrote, channels
//! unchanged, and the same in Pages.

use std::path::{Path, PathBuf};

use iwork::drawable::{Color, Fill, Outline};
use iwork::{Document, Kind};

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(name);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&path);
    path
}

const RED: Color = Color {
    red: 0.83,
    green: 0.18,
    blue: 0.18,
    alpha: 1.0,
};
const BLUE: Color = Color {
    red: 0.11,
    green: 0.35,
    blue: 0.62,
    alpha: 1.0,
};
const BLACK: Color = Color {
    red: 0.0,
    green: 0.0,
    blue: 0.0,
    alpha: 1.0,
};

fn fill_of(doc: &Document, drawable: u64) -> Option<Fill> {
    doc.drawables()
        .into_iter()
        .find(|d| d.identifier == drawable)
        .and_then(|d| d.style)
        .and_then(|style| doc.object_style(style))
        .and_then(|style| style.fill)
}

/// Two shapes sharing a preset end up with a style each, and the one that was
/// shared is left exactly as it was.
#[test]
fn painting_a_drawable_gives_it_a_style_of_its_own() {
    let mut deck = Document::new(Kind::Keynote).unwrap();
    let slide = deck.slides()[0].identifier.to_string();
    let a = deck
        .add_shape(
            &slide,
            Outline::Rectangle,
            "Rot",
            (120.0, 200.0),
            (500.0, 300.0),
        )
        .unwrap();
    let b = deck
        .add_shape(
            &slide,
            Outline::Ellipse,
            "Blau",
            (760.0, 200.0),
            (400.0, 300.0),
        )
        .unwrap();

    let shared = deck
        .drawables()
        .into_iter()
        .find(|d| d.identifier == a)
        .and_then(|d| d.style)
        .expect("a shape points at a style");
    assert_eq!(
        deck.drawables()
            .into_iter()
            .find(|d| d.identifier == b)
            .and_then(|d| d.style),
        Some(shared),
        "both shapes start out sharing the theme's preset"
    );

    deck.set_object_fill(a, Some(RED)).unwrap();
    deck.set_object_fill(b, Some(BLUE)).unwrap();

    let style_of = |doc: &Document, id: u64| {
        doc.drawables()
            .into_iter()
            .find(|d| d.identifier == id)
            .and_then(|d| d.style)
            .unwrap()
    };
    let (sa, sb) = (style_of(&deck, a), style_of(&deck, b));
    assert_ne!(sa, shared, "the first paint moved it off the preset");
    assert_ne!(sb, shared);
    assert_ne!(sa, sb, "and each shape got its own");

    // The preset it was sharing is untouched: it still paints nothing.
    assert!(
        matches!(
            deck.object_style(shared).and_then(|s| s.fill),
            Some(Fill::None) | None
        ),
        "the theme preset is left alone"
    );

    match fill_of(&deck, a) {
        Some(Fill::Color(c)) => assert!((c.red - 0.83).abs() < 1e-6, "{c:?}"),
        other => panic!("expected red, got {other:?}"),
    }
    match fill_of(&deck, b) {
        Some(Fill::Color(c)) => assert!((c.blue - 0.62).abs() < 1e-6, "{c:?}"),
        other => panic!("expected blue, got {other:?}"),
    }
    assert!(deck.problems().is_empty(), "{:?}", deck.problems());
}

/// Painting the same drawable twice does not make a second style for it.
#[test]
fn painting_twice_reuses_the_style_it_already_owns() {
    let mut deck = Document::new(Kind::Keynote).unwrap();
    let slide = deck.slides()[0].identifier.to_string();
    let shape = deck
        .add_shape(
            &slide,
            Outline::Rectangle,
            "",
            (100.0, 100.0),
            (300.0, 300.0),
        )
        .unwrap();

    deck.set_object_fill(shape, Some(RED)).unwrap();
    let first = deck
        .drawables()
        .into_iter()
        .find(|d| d.identifier == shape)
        .and_then(|d| d.style)
        .unwrap();

    deck.set_object_stroke(shape, BLACK, 3.0).unwrap();
    deck.set_object_opacity(shape, 0.5).unwrap();
    deck.set_object_fill(shape, Some(BLUE)).unwrap();

    let second = deck
        .drawables()
        .into_iter()
        .find(|d| d.identifier == shape)
        .and_then(|d| d.style)
        .unwrap();
    assert_eq!(first, second, "one style, painted four times");

    let style = deck.object_style(second).expect("a style");
    assert!(matches!(style.fill, Some(Fill::Color(c)) if (c.blue - 0.62).abs() < 1e-6));
    assert_eq!(style.opacity, Some(0.5));
    assert_eq!(style.stroke.as_ref().map(|s| s.width), Some(3.0));
    // Three properties in the bag, and the count says three.
    assert_eq!(
        style.override_count,
        Some(3),
        "override_count follows the bag"
    );
    assert!(deck.problems().is_empty(), "{:?}", deck.problems());
}

/// Values that are not colours or fractions are refused by name.
#[test]
fn an_impossible_paint_is_refused() {
    let mut deck = Document::new(Kind::Keynote).unwrap();
    let slide = deck.slides()[0].identifier.to_string();
    let shape = deck
        .add_shape(
            &slide,
            Outline::Rectangle,
            "",
            (100.0, 100.0),
            (300.0, 300.0),
        )
        .unwrap();

    let error = deck.set_object_opacity(shape, 1.5).unwrap_err();
    assert_eq!(
        error.refusal(),
        Some(iwork::Refusal::UnwritableValue),
        "{error:?}"
    );
    let error = deck.set_object_stroke(shape, BLACK, -1.0).unwrap_err();
    assert_eq!(
        error.refusal(),
        Some(iwork::Refusal::UnwritableValue),
        "{error:?}"
    );

    // A table has no object style, and says so rather than inventing one.
    let mut sheet = Document::new_spreadsheet("Blatt", "Tabelle", 2, 2).unwrap();
    let table = sheet.tables().first().map(|t| t.identifier).unwrap();
    let error = sheet.set_object_fill(table, Some(RED)).unwrap_err();
    assert_eq!(error.refusal(), Some(iwork::Refusal::NotDrawn), "{error:?}");
}

/// **The acceptance test.** The app opens the document, saves it again, and
/// the colours are still there. Off unless `IWORK_APP_CHECK=1`.
#[test]
fn the_apps_keep_a_colour_this_crate_painted() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the resave");
        return;
    }
    let resave = |path: &Path| {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/resave.sh");
        let output = std::process::Command::new(&script)
            .arg(path)
            .output()
            .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
        assert!(
            output.status.success(),
            "the app would not resave {}:\n{}\n{}",
            path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    };

    // Keynote.
    let mut deck = Document::new(Kind::Keynote).unwrap();
    let slide = deck.slides()[0].identifier.to_string();
    let shape = deck
        .add_shape(
            &slide,
            Outline::Rectangle,
            "Rot",
            (120.0, 200.0),
            (500.0, 300.0),
        )
        .unwrap();
    deck.set_object_fill(shape, Some(RED)).unwrap();
    deck.set_object_stroke(shape, BLACK, 3.0).unwrap();
    deck.set_object_opacity(shape, 0.9).unwrap();
    let out = scratch("iwork-painted.key");
    deck.save(&out).unwrap();
    resave(&out);

    let after = Document::open(&out).unwrap();
    match fill_of(&after, shape) {
        Some(Fill::Color(c)) => {
            assert!((c.red - 0.83).abs() < 1e-4, "Keynote kept the red: {c:?}");
            assert!((c.green - 0.18).abs() < 1e-4, "{c:?}");
        }
        other => panic!("Keynote dropped the fill: {other:?}"),
    }
    assert!(after.problems().is_empty(), "{:?}", after.problems());
    let _ = std::fs::remove_file(&out);

    // Pages.
    let mut doc = Document::new(Kind::Pages).unwrap();
    let circle = doc
        .add_shape(
            "page 1",
            Outline::Ellipse,
            "Kreis",
            (72.0, 300.0),
            (200.0, 200.0),
        )
        .unwrap();
    doc.set_object_fill(circle, Some(RED)).unwrap();
    doc.set_object_stroke(circle, BLACK, 2.0).unwrap();
    let out = scratch("iwork-painted.pages");
    doc.save(&out).unwrap();
    resave(&out);

    let after = Document::open(&out).unwrap();
    match fill_of(&after, circle) {
        Some(Fill::Color(c)) => assert!((c.red - 0.83).abs() < 1e-4, "Pages kept the red: {c:?}"),
        other => panic!("Pages dropped the fill: {other:?}"),
    }
    assert!(after.problems().is_empty(), "{:?}", after.problems());
    let _ = std::fs::remove_file(&out);
}

// -- gradients, shadows, pictures ---------------------------------------------

/// A 32 × 24 PNG, as in `drawable_write.rs`.
fn png() -> Vec<u8> {
    const BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAACAAAAAYCAIAAAAUMWhjAAAAJElEQVR4nGO4o6FBU8QwasGo\
        BaMWjFowasGoBaMWjFowNCwAAEUJhC7iQqksAAAAAElFTkSuQmCC";
    let mut out = Vec::new();
    let mut bits = 0u32;
    let mut held = 0u32;
    for byte in BASE64.bytes().filter(|b| !b.is_ascii_whitespace()) {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        bits = (bits << 6) | u32::from(value);
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
        }
    }
    out
}

/// A deck with one shape of each look this file is about.
fn looks() -> (Document, [u64; 3]) {
    use iwork::drawable::{Gradient, ImageFit, Shadow};
    let mut deck = Document::new(Kind::Keynote).unwrap();
    deck.slide_mut(0)
        .unwrap()
        .background_gradient(&Gradient::linear(BLUE, BLACK, 270.0))
        .unwrap();
    let slide = deck.slides()[0].identifier.to_string();
    let mut shape = |deck: &mut Document, x: f32| {
        deck.add_shape(&slide, Outline::Rectangle, "", (x, 200.0), (300.0, 200.0))
            .unwrap()
    };
    let graded = shape(&mut deck, 100.0);
    deck.set_object_gradient(
        graded,
        &Gradient {
            stops: vec![(RED, 0.0), (BLUE, 0.4), (BLACK, 1.0)],
            angle: 90.0,
        },
    )
    .unwrap();
    let shaded = shape(&mut deck, 500.0);
    deck.set_object_fill(shaded, Some(RED)).unwrap();
    deck.set_object_shadow(
        shaded,
        Some(Shadow {
            offset: 20.0,
            radius: 9,
            opacity: 0.5,
            ..Shadow::default()
        }),
    )
    .unwrap();
    let pictured = shape(&mut deck, 900.0);
    deck.set_object_image_fill(pictured, &png(), "fill.png", ImageFit::ScaleToFill)
        .unwrap();
    // The same picture again is the same stored file.
    let again = shape(&mut deck, 1300.0);
    deck.set_object_image_fill(again, &png(), "other-name.png", ImageFit::Tile)
        .unwrap();
    (deck, [graded, shaded, pictured])
}

fn style_of(doc: &Document, drawable: u64) -> iwork::drawable::ObjectStyle {
    let style = doc.drawable(drawable).and_then(|d| d.style).unwrap();
    doc.object_style(style).unwrap()
}

fn assert_looks(doc: &Document, [graded, shaded, pictured]: [u64; 3], who: &str) {
    match style_of(doc, graded).fill {
        Some(Fill::Gradient { stops: 3, angle }) => {
            let angle = angle.expect("an angle");
            assert!(
                (angle - std::f32::consts::FRAC_PI_2).abs() < 1e-4,
                "{who}: {angle}"
            );
        }
        other => panic!("{who}: the gradient is {other:?}"),
    }
    let shadow = style_of(doc, shaded).shadow.expect("a shadow");
    assert!(shadow.enabled, "{who}");
    assert_eq!((shadow.offset, shadow.radius), (20.0, 9), "{who}");
    assert!((shadow.opacity - 0.5).abs() < 1e-4, "{who}");
    match style_of(doc, pictured).fill {
        Some(Fill::Image {
            data: Some(data),
            technique: 3,
        }) => {
            assert!(
                doc.data_files().iter().any(|file| file.identifier == data),
                "{who}"
            );
        }
        other => panic!("{who}: the image fill is {other:?}"),
    }
    assert!(doc.problems().is_empty(), "{who}: {:?}", doc.problems());
}

/// A gradient, a shadow and a picture are each a property of the drawable's
/// own style, shaped as a theme's presets shape them; the picture is stored
/// once however often it is used, and the style that names it declares it.
#[test]
fn a_shape_takes_a_gradient_a_shadow_and_a_picture() {
    let (deck, shapes) = looks();
    assert_looks(&deck, shapes, "as built");
    let pictures = deck
        .data_files()
        .iter()
        .filter(|file| file.stored_name.ends_with(".png"))
        .count();
    assert_eq!(pictures, 1, "one stored file for one picture");

    let out = scratch("iwork-looks-offline.key");
    deck.save(&out).unwrap();
    assert_looks(&Document::open(&out).unwrap(), shapes, "as saved");

    // And what is not a gradient or a shadow is refused, not written.
    use iwork::drawable::{Gradient, Shadow};
    let (mut deck, [graded, ..]) = looks();
    let one_stop = Gradient {
        stops: vec![(RED, 0.0)],
        angle: 0.0,
    };
    assert!(deck.set_object_gradient(graded, &one_stop).is_err());
    let backwards = Gradient {
        stops: vec![(RED, 0.9), (BLUE, 0.1)],
        angle: 0.0,
    };
    assert!(deck.set_object_gradient(graded, &backwards).is_err());
    let loud = Shadow {
        opacity: 3.0,
        ..Shadow::default()
    };
    assert!(deck.set_object_shadow(graded, Some(loud)).is_err());
    // No shadow is a shadow that is switched off.
    deck.set_object_shadow(graded, None).unwrap();
    assert!(!style_of(&deck, graded).shadow.unwrap().enabled);
}

/// **Keynote keeps all three through its own save** — and the slide's
/// gradient background with them. Off unless `IWORK_APP_CHECK=1`.
#[test]
fn keynote_keeps_a_gradient_a_shadow_and_a_picture() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the resave");
        return;
    }
    let (deck, shapes) = looks();
    let out = scratch("iwork-looks.key");
    deck.save(&out).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/resave.sh");
    let output = std::process::Command::new(&script)
        .arg(&out)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", script.display()));
    assert!(
        output.status.success(),
        "Keynote would not resave {}:\n{}\n{}",
        out.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let after = Document::open(&out).unwrap();
    assert_looks(&after, shapes, "after Keynote's save");
    // The slide's style, or one up its chain, still holds two gradient stops.
    let slide = after.slides()[0].identifier;
    let mut at = iwork::style::reference_at(&after.archive(slide).unwrap(), &[1, 1]);
    let mut background = 0;
    for _ in 0..4 {
        let Some(id) = at else { break };
        let archive = after.archive(id).unwrap();
        if let Some(iwork::pb::Value::Bytes(raw)) = iwork::style::get_path(&archive, &[11, 1, 2]) {
            background = iwork::pb::Message::decode(&raw).unwrap().all(2).count();
            break;
        }
        at = iwork::style::reference_at(&archive, &[1, 3, 1]);
    }
    assert_eq!(
        background, 2,
        "the slide's gradient background after Keynote's save"
    );
}
