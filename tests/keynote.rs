//! The Keynote show: what a deck holds, and what the four writes do to it.
//!
//! Three claims, in the order they have to hold.
//!
//! **The deck this crate reads is the deck the app sees.** Behind
//! `IWORK_APP_CHECK=1`, `scripts/slide-oracle.sh` asks Keynote for the slide
//! count, the layout count, the slide size, every layout's name, and every
//! slide's number, base layout, skipped flag, title, body, presenter notes and
//! transition — and every one of those is compared. Keynote's dictionary is the
//! richest of the three and this is the phase that can afford to use all of it.
//!
//! **The shape is the shape in every deck.** A slide is its own component; a
//! placeholder's kind matches the field that names it; a note is a storage of
//! kind 4; the layouts a slide can be built on are the ones the theme lists.
//! Asserted over every `.key` in the corpus, not just the two the oracle runs
//! against.
//!
//! **A copied slide is a copied component, and the app opens it.** The
//! duplicate is the write that could break a document quietly, so it is checked
//! three ways: `iwork check` over the result, the two slides proved independent
//! by editing one, and — behind `IWORK_APP_CHECK=1` — Keynote asked to open the
//! deck and read the copy's title and notes back, then asked to *save* it, so
//! that what is on disk afterwards was written by Keynote from its own model.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use iwork::keynote::{PlaceholderKind, Role};
use iwork::Document;

fn generated(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/generated")
        .join(name);
    path.exists().then_some(path)
}

macro_rules! fixture {
    ($name:expr) => {
        match generated($name) {
            Some(path) => path,
            None => {
                eprintln!("no {} — skipping (run scripts/make-fixtures.sh)", $name);
                return;
            }
        }
    };
}

/// A password-protected package, whose object streams are ciphertext and which
/// `Document::open` refuses by design. Recognised by shape, not by name.
fn encrypted(path: &Path) -> bool {
    iwork::Package::read(path).is_ok_and(|package| package.contains(".iwpv2"))
}

fn keynote_fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/generated");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("key"))
        .filter(|path| !encrypted(path))
        .collect();
    found.sort();
    found
}

fn script(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join(name)
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(name);
    let _ = std::fs::remove_file(&path);
    path
}

// -- reading -----------------------------------------------------------------

/// Only Keynote has a show. A Pages or Numbers document must come back with
/// nothing rather than with something invented out of a colliding type number:
/// Numbers numbers its document archive 1 as well.
#[test]
fn only_a_keynote_document_has_a_show() {
    for name in ["numbers-values.numbers", "pages-report.pages"] {
        let Some(path) = generated(name) else {
            continue;
        };
        let doc = Document::open(&path).unwrap();
        assert!(doc.show().is_none(), "{name} reports a show");
        assert!(doc.slides().is_empty(), "{name} reports slides");
        assert!(doc.slide_layouts().is_empty(), "{name} reports layouts");
    }
}

/// Every deck: a theme with layouts, a slide size, and at least one slide.
#[test]
fn every_deck_has_a_theme_a_size_and_slides() {
    let decks = keynote_fixtures();
    assert!(!decks.is_empty(), "no .key fixtures at all");
    for path in decks {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let doc = Document::open(&path).unwrap();
        let show = doc.show().unwrap_or_else(|| panic!("{name}: no show"));
        assert!(!show.slides.is_empty(), "{name}: no slides");
        assert!(!show.layouts.is_empty(), "{name}: no slide layouts");
        assert!(
            show.width > 0.0 && show.height > 0.0,
            "{name}: slide size is {} × {}",
            show.width,
            show.height
        );
        assert!(!show.theme_name.is_empty(), "{name}: the theme has no name");
    }
}

/// The rules `iwork check` enforces, asserted directly so a failure names the
/// rule rather than a line of output: a layout that resolves, a placeholder
/// whose kind matches its field, a slide whose objects are its own.
#[test]
fn every_slide_keeps_the_shape_the_check_looks_for() {
    let mut slides = 0usize;
    for path in keynote_fixtures() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let doc = Document::open(&path).unwrap();
        let show = doc.show().unwrap();
        let layouts: BTreeSet<u64> = show.layouts.iter().map(|l| l.identifier).collect();
        let mut streams: BTreeSet<String> = BTreeSet::new();
        for slide in &show.slides {
            slides += 1;
            assert!(
                layouts.contains(&slide.layout.unwrap_or(0)),
                "{name}: slide {} is built on {:?}, which the theme does not list",
                slide.identifier,
                slide.layout
            );
            assert!(
                streams.insert(slide.stream.clone()),
                "{name}: two slides in {}",
                slide.stream
            );
            for (place, wanted) in [
                (&slide.title, PlaceholderKind::Title),
                (&slide.body, PlaceholderKind::Body),
                (&slide.slide_number, PlaceholderKind::SlideNumber),
                (&slide.object, PlaceholderKind::Object),
            ] {
                if let Some(place) = place {
                    assert_eq!(
                        place.kind,
                        wanted,
                        "{name}: slide {} names {} as its {}",
                        slide.identifier,
                        place.identifier,
                        wanted.as_str()
                    );
                }
            }
            for text in &slide.texts {
                let (stream, _) = doc.object(text.storage).unwrap();
                assert_eq!(
                    stream, slide.stream,
                    "{name}: slide {} owns storage {} in another component",
                    slide.identifier, text.storage
                );
            }
        }
    }
    assert!(slides >= 15, "only {slides} slides were checked");
}

/// Presenter notes are a storage of kind 4, reached only through
/// `KN.NoteArchive`. Nothing else in a deck is kind 4.
#[test]
fn presenter_notes_are_the_only_storages_of_kind_four() {
    let mut notes = 0usize;
    for path in keynote_fixtures() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let doc = Document::open(&path).unwrap();
        let show = doc.show().unwrap();
        let claimed: BTreeSet<u64> = show.slides.iter().filter_map(|s| s.note_storage).collect();
        let of_kind_four: BTreeSet<u64> = doc
            .storages()
            .into_iter()
            .filter(|s| s.kind == 4)
            .map(|s| s.identifier)
            .collect();
        assert_eq!(
            claimed, of_kind_four,
            "{name}: the note storages and the kind-4 storages are different sets"
        );
        notes += claimed.len();
    }
    assert!(notes >= 15, "only {notes} note storages were checked");
}

/// A skipped slide has no number, and the rest are numbered 1, 2, 3 … around
/// it. That is not a field: it is arithmetic over the deck, and the app agrees
/// by answering -1 for the skipped ones.
#[test]
fn numbering_counts_past_the_skipped_slides() {
    let path = fixture!("keynote-slides.key");
    let doc = Document::open(&path).unwrap();
    let show = doc.show().unwrap();
    let numbers: Vec<Option<usize>> = show.slides.iter().map(|s| s.number).collect();
    assert_eq!(
        numbers,
        vec![
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
            Some(6),
            None,
            None
        ],
        "the two skipped slides should have no number and not consume one"
    );
}

/// "Showing" is ownership. The app reports `title showing` false for a slide
/// whose field 5 still names a placeholder with text in it; what it means is
/// that the placeholder is not in `owned_drawables`.
#[test]
fn a_placeholder_is_shown_when_the_slide_owns_it() {
    let path = fixture!("keynote-deck.key");
    let doc = Document::open(&path).unwrap();
    let show = doc.show().unwrap();
    let statement = show
        .slides
        .iter()
        .find(|s| s.layout_name == "Statement")
        .expect("keynote-deck has a Statement slide");
    let title = statement.title.as_ref().expect("…which has a title field");
    assert!(
        !title.text.is_empty(),
        "the placeholder holds text the app does not draw"
    );
    assert!(!statement.title_showing(), "and is not shown");
    assert!(
        !statement.drawables.contains(&title.identifier),
        "because the slide does not own it"
    );
    assert!(statement.body_showing(), "while its body is shown");
}

/// The transition identifiers are the ones the app's dictionary lists, which is
/// what makes this an inventory rather than a guess.
#[test]
fn transitions_read_as_the_dictionary_names_them() {
    let path = fixture!("keynote-slides.key");
    let doc = Document::open(&path).unwrap();
    let effects: Vec<String> = doc
        .slides()
        .iter()
        .map(|s| s.transition.effect.clone())
        .collect();
    assert_eq!(
        effects,
        vec![
            "none",
            "apple:dissolve",
            "apple:push",
            "apple:magic-move-implied-motion-path",
            "apple:wipe",
            "com.apple.iWork.Keynote.KLNConfetti",
            "none",
            "none",
        ]
    );
    let push = &doc.slides()[2].transition;
    assert!(push.automatic, "slide 3 advances by itself");
    assert_eq!(push.delay, 3.0);
    assert_eq!(push.duration, 2.0);
    assert!(doc.slides()[0].transition.is_none());
}

/// The document-level "slide numbers showing" is not a document field: the app
/// writes `isSlideNumberVisible` on every node and leaves
/// `KN.ShowArchive.slideNumbersVisible` absent.
#[test]
fn slide_numbers_are_a_flag_on_every_node() {
    let on = fixture!("keynote-slides.key");
    let off = fixture!("keynote-deck.key");
    let with = Document::open(&on).unwrap().show().unwrap();
    let without = Document::open(&off).unwrap().show().unwrap();
    assert_eq!(with.numbers_shown_on(), with.slides.len());
    assert_eq!(without.numbers_shown_on(), 0);
    assert!(
        !with.slide_numbers_visible,
        "the show's own field stays absent even when the numbers are on"
    );
}

/// Every layout is named, and the theme lists them in the order the app numbers
/// them — which is what lets a slide's layout be reported by name.
#[test]
fn every_layout_has_a_name_and_a_place() {
    for path in keynote_fixtures() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let doc = Document::open(&path).unwrap();
        let show = doc.show().unwrap();
        for (index, layout) in show.layouts.iter().enumerate() {
            assert_eq!(layout.index, index, "{name}: layouts out of order");
            assert!(
                !layout.name.is_empty(),
                "{name}: layout {} has no name",
                layout.identifier
            );
            assert_eq!(
                layout.body_paragraph_styles.len(),
                5,
                "{name}: layout \"{}\" should have five outline levels",
                layout.name
            );
        }
    }
}

/// Roles are assigned once each. A storage that is a title is not also a text
/// box, and the free text box on a slide with placeholders is told apart from
/// them.
#[test]
fn a_storage_has_exactly_one_role() {
    let path = fixture!("keynote-slides.key");
    let doc = Document::open(&path).unwrap();
    let show = doc.show().unwrap();
    for slide in &show.slides {
        let mut seen: BTreeMap<u64, Role> = BTreeMap::new();
        for text in &slide.texts {
            assert!(
                seen.insert(text.storage, text.role).is_none(),
                "storage {} has two roles on slide {}",
                text.storage,
                slide.identifier
            );
        }
    }
    let free = show
        .slides
        .iter()
        .flat_map(|s| &s.texts)
        .find(|t| t.text == "Eine freie Textbox")
        .expect("keynote-slides has a text item that is not a placeholder");
    assert_eq!(free.role, Role::TextBox);
}

/// Every `KN` archive re-encodes to the bytes it was decoded from. The claim
/// this whole module stands on: a field this crate does not understand is a
/// field it does not lose.
#[test]
fn every_keynote_archive_re_encodes_to_itself() {
    let mut checked = 0usize;
    for path in keynote_fixtures() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let doc = Document::open(&path).unwrap();
        for (_, object) in doc.objects() {
            // The app-scoped range is the KN one, and the only one where a
            // Keynote number means something Keynote-specific.
            if object.message_type() >= 200 {
                continue;
            }
            let decoded = iwork::pb::Message::decode(object.payload())
                .unwrap_or_else(|e| panic!("{name}: object {}: {e}", object.identifier));
            assert_eq!(
                decoded.encode(),
                object.payload(),
                "{name}: object {} (type {}) does not re-encode to itself",
                object.identifier,
                object.message_type()
            );
            checked += 1;
        }
    }
    assert!(checked >= 100, "only {checked} KN archives were re-encoded");
}

// -- writing -----------------------------------------------------------------

/// Skipping is one varint on the node, and it rewrites one stream.
#[test]
fn skipping_a_slide_touches_the_document_stream_and_nothing_else() {
    let path = fixture!("keynote-deck.key");
    let mut doc = Document::open(&path).unwrap();
    let slide = doc.slides()[0].identifier;
    assert!(!doc.show().unwrap().slides[0].skipped);

    assert!(doc.set_slide_skipped(slide, true).unwrap());
    assert!(
        !doc.set_slide_skipped(slide, true).unwrap(),
        "setting it again changes nothing"
    );
    assert_eq!(doc.changed_streams(), vec!["Index/Document.iwa"]);

    let show = doc.show().unwrap();
    assert!(show.slides[0].skipped);
    assert_eq!(show.slides[0].number, None, "and it loses its number");
    assert_eq!(show.slides[1].number, Some(1), "which the next one takes");
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());

    assert!(doc.set_slide_skipped(slide, false).unwrap());
    assert!(
        doc.changed_streams().is_empty(),
        "and unskipping puts the bytes back exactly"
    );
}

/// Reordering is a permutation of the slide tree and touches nothing else — not
/// the nodes, not the slide components.
#[test]
fn moving_a_slide_permutes_the_tree_and_leaves_the_slides_alone() {
    let path = fixture!("keynote-deck.key");
    let mut doc = Document::open(&path).unwrap();
    let before: Vec<u64> = doc.slides().iter().map(|s| s.identifier).collect();
    let moved = before[0];

    assert_eq!(doc.move_slide(moved, 2).unwrap(), 2);
    assert_eq!(doc.changed_streams(), vec!["Index/Document.iwa"]);
    let after: Vec<u64> = doc.slides().iter().map(|s| s.identifier).collect();
    assert_eq!(
        after,
        vec![before[1], before[2], before[0], before[3], before[4], before[5]]
    );
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());

    assert_eq!(doc.move_slide(moved, 0).unwrap(), 0);
    assert!(
        doc.changed_streams().is_empty(),
        "moving it back reproduces the original bytes"
    );
}

/// Presenter notes are a storage, and writing one goes through the Phase 4
/// remapper like any other text.
#[test]
fn presenter_notes_are_written_like_any_other_text() {
    let path = fixture!("keynote-deck.key");
    let mut doc = Document::open(&path).unwrap();
    let slide = doc.slides()[0].identifier;
    doc.set_presenter_notes(slide, "Ganz neue Notizen mit Größe 🎬")
        .unwrap();
    assert_eq!(
        doc.show().unwrap().slides[0].notes,
        "Ganz neue Notizen mit Größe 🎬"
    );
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    assert_eq!(doc.changed_streams(), vec!["Index/Slide-2652176.iwa"]);
}

/// A slide with no notes is named rather than given some.
#[test]
fn a_slide_without_notes_refuses_rather_than_inventing_a_storage() {
    let path = fixture!("keynote-deck.key");
    let mut doc = Document::open(&path).unwrap();
    assert!(doc.set_presenter_notes(999_999_999, "…").is_err());
}

/// A layout is not a slide, and copying one is refused by name — Keynote's own
/// dictionary will not do it either.
#[test]
fn a_slide_layout_cannot_be_duplicated() {
    let path = fixture!("keynote-deck.key");
    let mut doc = Document::open(&path).unwrap();
    let layout = doc.slide_layouts()[0].identifier;
    let error = doc.duplicate_slide(layout).unwrap_err().to_string();
    assert!(error.contains("layout"), "{error}");
}

/// The copy is a whole component: its own stream, its own objects, its own
/// entry in the package metadata, and nothing shared with the original except
/// what the original itself shares — the layout, the stylesheet, the media.
#[test]
fn a_duplicated_slide_is_a_duplicated_component() {
    let path = fixture!("keynote-deck.key");
    let mut doc = Document::open(&path).unwrap();
    let source = doc.slides()[1].clone();
    let before = doc.slides().len();

    let copy = doc.duplicate_slide(source.identifier).unwrap();
    assert_eq!(copy.index, 2, "the copy goes straight after its original");
    assert_eq!(doc.slides().len(), before + 1);
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());

    let show = doc.show().unwrap();
    let made = &show.slides[2];
    assert_eq!(made.identifier, copy.identifier);
    assert_ne!(made.stream, source.stream, "its own component");
    assert_eq!(
        made.layout, source.layout,
        "the same layout as the original"
    );
    assert_eq!(made.title_text(), source.title_text());
    assert_eq!(made.body_text(), source.body_text());
    assert_eq!(made.notes, source.notes);
    assert_eq!(made.transition, source.transition);

    // Not one identifier in common: a copy that shared an object would edit
    // the original when the copy was edited.
    let mine: BTreeSet<u64> = made.texts.iter().map(|t| t.storage).collect();
    let theirs: BTreeSet<u64> = source.texts.iter().map(|t| t.storage).collect();
    assert!(
        mine.is_disjoint(&theirs),
        "the copy shares storages with the original: {:?}",
        mine.intersection(&theirs).collect::<Vec<_>>()
    );

    // Only three streams are rewritten: the new one, the document (node and
    // slide tree) and the metadata (the component entry).
    let mut changed = doc.changed_streams();
    changed.sort_unstable();
    assert_eq!(
        changed,
        vec![
            "Index/Document.iwa",
            "Index/Metadata.iwa",
            copy.stream.as_str()
        ]
    );

    // And the two are independent.
    doc.set_text(
        made.title.as_ref().unwrap().storage.unwrap(),
        "Nur die Kopie",
    )
    .unwrap();
    let show = doc.show().unwrap();
    assert_eq!(show.slides[2].title_text(), "Nur die Kopie");
    assert_eq!(show.slides[1].title_text(), source.title_text());
}

/// The identifiers a copy takes are above the package's high-water mark, and
/// the mark moves with them — otherwise iWork would hand the same number out
/// again and two objects would be one.
#[test]
fn a_copy_allocates_above_the_high_water_mark() {
    let path = fixture!("keynote-deck.key");
    let mut doc = Document::open(&path).unwrap();
    let mark = doc.last_object_identifier().unwrap();
    let copy = doc.duplicate_slide(doc.slides()[1].identifier).unwrap();
    assert!(copy.identifier > mark);
    assert!(doc.last_object_identifier().unwrap() >= copy.node);
    assert!(
        doc.next_object_identifier() > copy.node,
        "and the next allocation is above the copy"
    );
}

/// A copied image slide shares the original's media rather than growing the
/// package — which is what the app's own duplicate does.
#[test]
fn a_copied_image_slide_shares_its_media() {
    let path = fixture!("keynote-deck.key");
    let mut doc = Document::open(&path).unwrap();
    let with_media = doc
        .slides()
        .iter()
        .find(|s| {
            s.drawables
                .iter()
                .any(|d| doc.drawable(*d).is_some_and(|d| d.media.is_some()))
        })
        .expect("keynote-deck has an image slide")
        .identifier;
    let files = doc.data_files().len();
    let copy = doc.duplicate_slide(with_media).unwrap();
    assert!(copy.media > 0, "the copy declares the media it uses");
    assert_eq!(
        doc.data_files().len(),
        files,
        "and no Data/ entry was added for it"
    );
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// Two copies of the same slide are two slides, not one — the second must not
/// reuse the first's identifiers or its stream.
#[test]
fn a_slide_can_be_copied_twice() {
    let path = fixture!("keynote-deck.key");
    let mut doc = Document::open(&path).unwrap();
    let source = doc.slides()[1].identifier;
    let first = doc.duplicate_slide(source).unwrap();
    let second = doc.duplicate_slide(source).unwrap();
    assert_ne!(first.identifier, second.identifier);
    assert_ne!(first.stream, second.stream);
    assert_eq!(doc.slides().len(), 8);
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
}

/// The duplicate is reproducible: the same document copied the same way twice
/// gives the same bytes. Object UUIDs are derived rather than drawn for exactly
/// this reason.
#[test]
fn duplicating_a_slide_is_reproducible() {
    let path = fixture!("keynote-deck.key");
    let out_one = temp("iwork-kn-dup-1.key");
    let out_two = temp("iwork-kn-dup-2.key");
    for out in [&out_one, &out_two] {
        let mut doc = Document::open(&path).unwrap();
        doc.duplicate_slide(doc.slides()[1].identifier).unwrap();
        doc.save(out).unwrap();
    }
    assert_eq!(
        std::fs::read(&out_one).unwrap(),
        std::fs::read(&out_two).unwrap(),
        "two identical duplicates produced different files"
    );
    let _ = std::fs::remove_file(&out_one);
    let _ = std::fs::remove_file(&out_two);
}

// -- the app -----------------------------------------------------------------

/// Ask Keynote for the deck: `show`, `layout` and `slide` records, tab
/// separated, as `scripts/applescript/slide-oracle.applescript` documents.
fn ask_the_app(path: &Path) -> Vec<Vec<String>> {
    let output = std::process::Command::new(script("slide-oracle.sh"))
        .arg(path)
        .output()
        .unwrap_or_else(|e| panic!("slide-oracle.sh: {e}"));
    assert!(
        output.status.success(),
        "slide-oracle.sh failed for {path:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect()
}

/// The app escapes tabs and line breaks inside a text so that one slide is one
/// line; this puts them back. Carriage return and linefeed stay apart, because
/// **Keynote separates a body's paragraphs with a carriage return** and folding
/// the two together would hide whether this crate reads that correctly.
fn unescape(text: &str) -> String {
    text.replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
}

/// Everything Keynote will say about a deck, compared with everything this
/// crate says about it. Off unless `IWORK_APP_CHECK=1`.
#[test]
fn the_app_agrees_about_every_slide() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }
    let mut compared = 0usize;
    for name in ["keynote-deck.key", "keynote-slides.key"] {
        let Some(path) = generated(name) else {
            eprintln!("no {name} — skipping");
            continue;
        };
        let said = ask_the_app(&path);
        let doc = Document::open(&path).unwrap();
        let show = doc.show().unwrap();

        let header = said.iter().find(|r| r[0] == "show").expect("a show record");
        assert_eq!(
            header[1].parse::<usize>().unwrap(),
            show.slides.len(),
            "{name}: slide count"
        );
        assert_eq!(
            header[2].parse::<usize>().unwrap(),
            show.layouts.len(),
            "{name}: layout count"
        );
        assert_eq!(
            header[3].parse::<f32>().unwrap(),
            show.width,
            "{name}: slide width"
        );
        assert_eq!(
            header[4].parse::<f32>().unwrap(),
            show.height,
            "{name}: slide height"
        );
        assert_eq!(
            header[5] == "true",
            show.numbers_shown_on() == show.slides.len(),
            "{name}: slide numbers showing"
        );

        let layouts: Vec<&Vec<String>> = said.iter().filter(|r| r[0] == "layout").collect();
        assert_eq!(layouts.len(), show.layouts.len());
        for (record, layout) in layouts.iter().zip(&show.layouts) {
            assert_eq!(
                record[1].parse::<usize>().unwrap(),
                layout.index + 1,
                "{name}: layout order"
            );
            assert_eq!(record[2], layout.name, "{name}: layout name");
            compared += 1;
        }

        let slides: Vec<&Vec<String>> = said.iter().filter(|r| r[0] == "slide").collect();
        assert_eq!(slides.len(), show.slides.len(), "{name}: slide count");
        for (record, slide) in slides.iter().zip(&show.slides) {
            let number: i64 = record[1].parse().unwrap();
            assert_eq!(
                number,
                slide.number.map(|n| n as i64).unwrap_or(-1),
                "{name}: slide {} number",
                slide.identifier
            );
            assert_eq!(
                record[2], slide.layout_name,
                "{name}: slide {} layout",
                slide.identifier
            );
            assert_eq!(
                record[3] == "true",
                slide.skipped,
                "{name}: slide {} skipped",
                slide.identifier
            );
            assert_eq!(
                record[4] == "true",
                slide.title_showing(),
                "{name}: slide {} title showing",
                slide.identifier
            );
            assert_eq!(
                record[5] == "true",
                slide.body_showing(),
                "{name}: slide {} body showing",
                slide.identifier
            );
            if record[6] != "-" {
                assert_eq!(
                    unescape(&record[6]),
                    slide.title_text(),
                    "{name}: slide {} title",
                    slide.identifier
                );
            }
            if record[7] != "-" {
                assert_eq!(
                    unescape(&record[7]),
                    slide.body_text(),
                    "{name}: slide {} body",
                    slide.identifier
                );
            }
            assert_eq!(
                unescape(&record[8]),
                slide.notes,
                "{name}: slide {} presenter notes",
                slide.identifier
            );
            // The app names an effect in English; the document names it by
            // identifier. Only "no transition effect" can be compared without
            // a table, and it is the one that matters for an inventory.
            assert_eq!(
                record[9] == "no transition effect",
                slide.transition.is_none(),
                "{name}: slide {} transition",
                slide.identifier
            );
            if !slide.transition.is_none() {
                assert_eq!(
                    record[12].parse::<f64>().unwrap(),
                    slide.transition.duration,
                    "{name}: slide {} transition duration",
                    slide.identifier
                );
                assert_eq!(
                    record[10] == "true",
                    slide.transition.automatic,
                    "{name}: slide {} automatic transition",
                    slide.identifier
                );
            }
            compared += 1;
        }
    }
    assert!(compared >= 40, "only {compared} records were compared");
}

/// The four writes, put through Keynote and read back. Off unless
/// `IWORK_APP_CHECK=1`.
///
/// One document carries all four so that one pass of the app answers for all of
/// them, and so that they are shown not to interfere: the copy is made, then a
/// slide is skipped, another unskipped, one moved and the notes rewritten.
#[test]
fn keynote_reads_back_every_write() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }
    let path = fixture!("keynote-deck.key");
    let out = temp("iwork-keynote-writes.key");

    let mut doc = Document::open(&path).unwrap();
    let slides: Vec<u64> = doc.slides().iter().map(|s| s.identifier).collect();
    let skipped = doc.slides().iter().find(|s| s.skipped).unwrap().identifier;

    let copy = doc.duplicate_slide(slides[1]).unwrap();
    doc.set_presenter_notes(copy.identifier, "Notizen nur auf der Kopie.")
        .unwrap();
    doc.set_text(
        doc.show()
            .unwrap()
            .slide(copy.identifier)
            .unwrap()
            .title
            .as_ref()
            .unwrap()
            .storage
            .unwrap(),
        "Kopie mit eigenem Titel",
    )
    .unwrap();
    doc.set_slide_skipped(slides[0], true).unwrap();
    doc.set_slide_skipped(skipped, false).unwrap();
    doc.move_slide(slides[5], 0).unwrap();
    assert!(doc.problems().is_empty(), "{:?}", doc.problems());
    doc.save(&out).unwrap();

    let said = ask_the_app(&out);
    let slides: Vec<&Vec<String>> = said.iter().filter(|r| r[0] == "slide").collect();
    assert_eq!(slides.len(), 7, "Keynote should see one more slide");

    let doc = Document::open(&out).unwrap();
    let show = doc.show().unwrap();
    for (record, slide) in slides.iter().zip(&show.slides) {
        assert_eq!(record[2], slide.layout_name);
        assert_eq!(record[3] == "true", slide.skipped);
        assert_eq!(unescape(&record[8]), slide.notes);
    }
    assert!(
        slides
            .iter()
            .any(|r| r[6] == "Kopie mit eigenem Titel" && r[8] == "Notizen nur auf der Kopie."),
        "the copy did not come back with its own title and notes"
    );
    assert!(
        slides
            .iter()
            .any(|r| r[6] == "Zahlen" && r[8] == "Hier langsam sprechen."),
        "and the original did not keep its own"
    );
    let _ = std::fs::remove_file(&out);
}

/// The deck with the copied slide, opened by Keynote and **saved** by it. What
/// is on disk afterwards was written by the app from its own model, so a
/// component it could not load would not come back. Off unless
/// `IWORK_APP_CHECK=1`.
#[test]
fn keynote_saves_the_deck_with_the_slide_this_crate_copied() {
    if std::env::var("IWORK_APP_CHECK").as_deref() != Ok("1") {
        eprintln!("IWORK_APP_CHECK is not 1 — skipping the app round trip");
        return;
    }
    let path = fixture!("keynote-slides.key");
    let out = temp("iwork-keynote-resave.key");

    let mut doc = Document::open(&path).unwrap();
    let source = doc.slides()[1].clone();
    let copy = doc.duplicate_slide(source.identifier).unwrap();
    doc.save(&out).unwrap();

    let status = std::process::Command::new(script("resave.sh"))
        .arg(&out)
        .status()
        .unwrap_or_else(|e| panic!("resave.sh: {e}"));
    assert!(status.success(), "Keynote would not open and save {out:?}");

    let after = Document::open(&out).unwrap();
    let show = after.show().unwrap();
    assert_eq!(show.slides.len(), 9, "Keynote dropped the copied slide");
    let made = show
        .slide(copy.identifier)
        .expect("and it kept the copy's identifier");
    assert_eq!(made.title_text(), source.title_text());
    assert_eq!(made.notes, source.notes);
    assert_eq!(made.layout_name, source.layout_name);
    assert!(after.problems().is_empty(), "{:?}", after.problems());
    let _ = std::fs::remove_file(&out);
}
