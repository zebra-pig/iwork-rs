//! A Keynote deck about *The European Correspondent*, written by this crate.
//!
//!     cargo run --example european-correspondent -- --print
//!     cargo run --example european-correspondent -- Base.key The-European-Correspondent.key
//!
//! **A deck is authored from a base deck, never from nothing.** Ground rule 3
//! of [`PLAN.md`](../PLAN.md) — copy, don't synthesise — applies to a slide as
//! much as to a style: a slide is a whole component, and a theme is hundreds of
//! objects of stylesheet nobody here can invent. So this example takes a `.key`
//! that already exists, uses its slides as prototypes, and writes the content
//! into them:
//!
//!   * the deck is grown to the length of [`DECK`] with
//!     [`Document::duplicate_slide`], copying the base's last title-and-body
//!     slide once per slide still needed, each copy moved to the end;
//!   * each slide's title and body placeholders are written with
//!     [`Document::set_text`], the bullets joined by `\r`, which is what a
//!     paragraph break is in a `TSWP.StorageArchive` (`\n` would be one
//!     paragraph containing a newline — see [`FORMAT.md`](../FORMAT.md) §Text);
//!   * presenter notes go through [`Document::set_presenter_notes`], which
//!     keeps the node's `hasNote` flag in step;
//!   * a base slide the deck does not need is *skipped* rather than deleted,
//!     because there is no `delete-slide` and this example will not pretend
//!     otherwise;
//!   * the result is written with [`Document::save_as_new`], so the deck is a
//!     document of its own rather than another version of the base.
//!
//! `scripts/european-correspondent.sh` makes the base deck with Keynote and
//! runs this; with no Keynote, hand it any `.key` you have.
//!
//! Two limits worth knowing before looking at the output. **A slide's layout
//! cannot be changed** — Keynote's own dictionary makes `slide layout`
//! read-only — so every slide here wears the layout of the prototype it was
//! copied from; a base deck whose second slide is "Title & Bullets" gives a
//! deck of bullets. And **nothing here does layout**: text longer than the
//! placeholder is text the app will shrink or overflow, and this crate cannot
//! see which.
//!
//! The content is the outlet's own published material, retrieved 19 August
//! 2026; the last slide carries the sources.

use iwork::{Document, Kind, Slide};
use std::collections::BTreeSet;

/// One slide's worth of content.
struct Entry {
    title: &'static str,
    /// One paragraph each, in the body placeholder.
    bullets: &'static [&'static str],
    /// Presenter notes — what is said, not what is shown.
    notes: &'static str,
}

/// The deck. Slide 1 is the title slide; the rest are bullets.
const DECK: &[Entry] = &[
    Entry {
        title: "The European Correspondent",
        bullets: &[
            "Daily journalism from every country in Europe",
            "A non-profit newsroom, launched November 2022",
            "A briefing — August 2026",
        ],
        notes: "The pitch in one line: a daily newsletter about the whole continent, \
                written by correspondents who live in it, free to read. Every figure in \
                this deck is the outlet's own published number, retrieved in August 2026 \
                — the last slide has the links.",
    },
    Entry {
        title: "Europe has no European newspaper",
        bullets: &[
            "Local papers, national papers — and nothing spanning the continent",
            "Migration, AI, the climate crisis, the war in Ukraine: shared problems",
            "A tactic the far right tries in Austria reaches the UK within weeks",
            "The EU shapes daily life and hides behind its own complexity",
        ],
        notes: "This is the founders' own framing, from the 2025 essay \"Journalism for \
                Europe\": once you notice that no popular newspaper spans Europe, you \
                cannot unsee it. Their manifesto puts it more sharply — the rich and \
                powerful are organised internationally, civil society is not.",
    },
    Entry {
        title: "What it is",
        bullets: &[
            "A daily newsletter: three to five stories, with the data behind them",
            "Free, no paywall — by email, in the app, on the website",
            "Two weeklies: the EU institutions, and the editor-in-chief's pick",
            "First edition 28 November 2022, and every day since",
        ],
        notes: "Analysis and context rather than breaking news, and written for readers \
                who do not work in Brussels for a living. The 2025 relaunch added a \
                searchable archive, filterable by date and country — the thing readers \
                asked for most.",
    },
    Entry {
        title: "How the newsroom works",
        bullets: &[
            "Correspondents in every European country — Lisbon to Yerevan",
            "Local expertise, written for a reader somewhere else on the continent",
            "Every country with a correspondent monthly; none more than weekly",
            "Context and classification, not sensation",
        ],
        notes: "The monthly-and-not-more-than-weekly rule is published in their FAQ, and \
                it is the whole editorial model in one sentence: it forces the small \
                countries onto the page and keeps the loud ones off it. The desks — \
                democracy and human rights, culture, economics, data — sit across that \
                country network.",
    },
    Entry {
        title: "The manifesto, in its own words",
        bullets: &[
            "\u{201c}In the 21st century, Europe must learn to be a continent\u{201d}",
            "\u{201c}We are aware of our biases and strive not for artificial \
             objectivity but fairness\u{201d}",
            "\u{201c}Journalism needs empathy \u{2013} towards the story and those who \
             experience it\u{201d}",
            "\u{201c}European journalism has not really been done before\u{201d}",
            "\u{201c}Everything we do, therefore, is an experiment\u{201d}",
        ],
        notes: "Quoted from the manifesto, which is the document the editorial policy \
                then builds on: accuracy, fairness, diversity, and the Society of \
                Professional Journalists' code as the framework. Worth reading aloud — \
                it is unusually plain for a mission statement.",
    },
    Entry {
        title: "From living rooms to Brussels",
        bullets: &[
            "Early 2022 — the idea; every funder they could reach says no",
            "28 November 2022 — the first daily edition, made by an unpaid team",
            "2023 — the European Charlemagne Youth Prize",
            "2024 — registered as an ASBL in Brussels, on top of the Basel association",
            "2025 — the first salaries, then the European Commission grant",
        ],
        notes: "Three years from Zoom calls to a newsroom that pays people. The legal \
                shape follows that history: the origin is a Verein in Basel, and the \
                Brussels ASBL at Rue des Tanneurs was added in 2024 when the work moved \
                to the EU.",
    },
    Entry {
        title: "Who makes it",
        bullets: &[
            "Carla Allenbach — managing director and co-founder",
            "Julius E. O. Fintelmann — editor-in-chief and co-founder",
            "Philippe Kramer — publisher and co-founder",
            "340+ journalists and team members worked for two and a half years unpaid",
            "Correspondents, editors, data journalists, an app developer",
        ],
        notes: "The three founders come out of Republik and EURACTIV, Handelsblatt, and \
                Swiss direct-democracy campaigning respectively. The number that carries \
                the story is the 340: a continental newsroom that existed for two and a \
                half years on volunteered evenings before it existed on a payroll.",
    },
    Entry {
        title: "Who reads it",
        bullets: &[
            "More than 80,000 newsletter subscribers",
            "Daily edition: 52,000 subscribers, 41% open rate",
            "EU-institutions weekly 13,000; the editor's pick 5,000 at 51%",
            "320,000 social followers, 165,000 of them on Instagram",
            "About 2,400 new readers every month",
        ],
        notes: "Open rates in the forties are strong for a free newsletter of this size. \
                The audience skews towards people who act on it — Commission staff, \
                national politicians and other journalists are on the published list of \
                readers who have written in.",
    },
    Entry {
        title: "Seven languages",
        bullets: &[
            "English since 2022, German since November 2025",
            "French, Spanish, Italian, Polish and Ukrainian through 2026",
            "\u{201c}Journalism can only reach every European if it is multilingual\u{201d}",
            "Reporting continues from outside the EU — paid for by readers, not the grant",
        ],
        notes: "All seven editions are live as of August 2026. The non-EU point is a real \
                constraint rather than a flourish: the Commission grant cannot pay team \
                members outside the Union, so the reporting from Türkiye, Norway or the \
                Caucasus is funded by donations or not at all.",
    },
    Entry {
        title: "How it is paid for",
        bullets: &[
            "€2.16 million from the European Commission, August 2025 to May 2027",
            "Independence written into the contract; newsroom kept apart from management",
            "€102,650 in reader donations since 2022 — €10.63 on average",
            "Advertising and services: 20+ clients, €38,000 — no fossil fuels, no tobacco",
            "Prizes and foundations: Charlemagne, JournalismAI, the ECF, the IPI",
        ],
        notes: "They publish the whole ledger, down to the average donation and the \
                1,000-copy print book that brought in €13,376. The advertising exclusions \
                cost them money and they say so — which is the point of putting the \
                policy in writing.",
    },
    Entry {
        title: "The 2027 clock",
        bullets: &[
            "The grant runs out in May 2027 — after that the outlet is on its own",
            "The target: about €1 million a year, the majority from readers",
            "Everything stays free; the money has to come from those who value it",
            "Two years to turn 80,000 readers into a revenue base",
        ],
        notes: "This is the interesting part for anyone thinking about media models. The \
                grant did not solve the problem, it bought two years to solve it — and \
                the condition they set themselves is that reader money, not \
                institutional money, has to be the majority by 2027.",
    },
    Entry {
        title: "What to take away",
        bullets: &[
            "A continental audience exists; it was found without a paywall",
            "Volunteers proved the idea, institutional money professionalised it",
            "Independence is a governance arrangement, not a promise",
            "The hard part is not the journalism — it is the €1 million",
        ],
        notes: "If you take one thing from this: the gap in the European media market was \
                demonstrated by 340 people working for free, and the open question is \
                whether a continent will pay for the journalism it says it wants.",
    },
    Entry {
        title: "Sources",
        bullets: &[
            "europeancorrespondent.com — manifesto, team, editorial policy, imprint",
            "\u{201c}Journalism for Europe\u{201d}, 10 June 2025 — the grant announcement",
            "\u{201c}How we finance our journalism\u{201d}, Brussels, 1 August 2025",
            "Audience and language figures from the services page and the editions",
            "All retrieved 19 August 2026",
        ],
        notes: "Everything in this deck is the outlet's own published material. Where a \
                page carries a date, that date is on the slide; nothing here is estimated \
                or rounded up.",
    },
];

fn main() -> std::process::ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["--print"] | [] => {
            print_deck();
            Ok(())
        }
        [base, out] => build(base, out),
        _ => {
            eprintln!(
                "usage: european-correspondent [--print | <base.key> <out.key>]\n\
                 \n\
                 The base deck supplies the theme, the layouts and the stylesheet; this\n\
                 example supplies the words. Any Keynote document works — one Keynote\n\
                 wrote from a theme is the obvious one, and\n\
                 scripts/european-correspondent.sh makes it for you."
            );
            return std::process::ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// The deck as text — what it says, without needing a `.key` to say it into.
fn print_deck() {
    for (index, entry) in DECK.iter().enumerate() {
        println!("{}. {}", index + 1, entry.title);
        for bullet in entry.bullets {
            println!("     • {bullet}");
        }
        println!("   notes: {}", entry.notes);
        println!();
    }
    println!("{} slides", DECK.len());
}

fn build(base: &str, out: &str) -> Result<(), iwork::Error> {
    let mut document = Document::open(base)?;
    if document.kind() != Kind::Keynote {
        return Err(iwork::Error::Format(format!(
            "{base} is a {} document — a deck needs a Keynote base",
            document.kind().as_str()
        )));
    }

    // What the base deck is already unhappy about, so that only what this run
    // breaks is this run's problem.
    let baseline: BTreeSet<String> = document.problems().into_iter().collect();

    // The prototype is the *last* slide that has both a title and a body
    // placeholder, which in a deck Keynote has just made from a theme is the
    // bullets slide rather than the title slide. Its layout is the one every
    // copied slide will wear, because a slide's layout cannot be changed after
    // the fact.
    let slides = document.slides();
    let prototype = slides
        .iter()
        .rposition(|slide| storage_of(slide.title.as_ref()).is_some() && body_of(slide).is_some())
        .ok_or_else(|| {
            iwork::Error::Format(format!(
                "{base} has no slide with both a title and a body placeholder — \
                 there is nothing to copy"
            ))
        })?;
    println!(
        "base: {} slide(s), prototype is slide {} on \"{}\"",
        slides.len(),
        prototype + 1,
        slides[prototype].layout_name
    );

    // Grow the deck to the length of the content. A copy lands straight after
    // its source, so each one is moved to the end; the deck is re-read every
    // time because both calls change what the positions mean.
    while document.slides().len() < DECK.len() {
        let source = document.slides()[prototype].identifier;
        let copy = document.duplicate_slide(source)?;
        let last = document.slides().len() - 1;
        document.move_slide(copy.identifier, last)?;
    }

    // Write the words.
    let slides = document.slides();
    for (entry, slide) in DECK.iter().zip(&slides) {
        fill(&mut document, slide, entry)?;
    }

    // A base deck longer than the content keeps its extra slides — there is no
    // `delete-slide`, for the reasons README's Keynote limits give — so they
    // are taken out of the show instead, and said so.
    for slide in slides.iter().skip(DECK.len()) {
        document.set_slide_skipped(slide.identifier, true)?;
        println!(
            "slide {}: not needed — skipped rather than deleted",
            slide.index + 1
        );
    }

    // The checker is not a substitute for opening the file — the README is
    // blunt about that — but it is the cheapest way to catch a deck that has
    // come apart, and it is used the way its own documentation asks: what
    // matters is the *difference* from the base deck. A complaint the base
    // already drew is a complaint about somebody else's file.
    let problems = document.problems();
    let broken: Vec<&String> = problems
        .iter()
        .filter(|problem| !baseline.contains(*problem))
        .collect();
    if !broken.is_empty() {
        for problem in &broken {
            eprintln!("check: {problem}");
        }
        return Err(iwork::Error::Format(format!(
            "{} problem(s) this run introduced — refusing to write the deck",
            broken.len()
        )));
    }
    if !baseline.is_empty() {
        println!(
            "check: {} problem(s) the base deck already had, and still has",
            baseline.len()
        );
    }

    // A new deck, not another version of the base: a fresh identity, with the
    // lineage kept.
    let identity = document.save_as_new(out)?;
    println!(
        "wrote {out} — {} slides, document {}",
        DECK.len(),
        identity.document_uuid
    );
    println!("Open it in Keynote before trusting it; nothing here has seen the app.");
    Ok(())
}

/// Put one entry on one slide, and report what the slide would not take.
fn fill(document: &mut Document, slide: &Slide, entry: &Entry) -> Result<(), iwork::Error> {
    let number = slide.index + 1;

    match storage_of(slide.title.as_ref()) {
        Some(storage) => {
            document.set_text(storage, entry.title)?;
            if !slide.title_showing() {
                println!("slide {number}: the title is written but the slide does not show it");
            }
        }
        None => println!(
            "slide {number}: no title placeholder — \"{}\" unplaced",
            entry.title
        ),
    }

    // `\r` is the paragraph separator iWork stores; one bullet per paragraph.
    let body = entry.bullets.join("\r");
    match body_of(slide) {
        Some(storage) => {
            document.set_text(storage, &body)?;
            if !slide.body_showing() {
                println!("slide {number}: the body is written but the slide does not show it");
            }
        }
        None if !entry.bullets.is_empty() => {
            println!(
                "slide {number}: no body placeholder — {} bullet(s) unplaced",
                entry.bullets.len()
            )
        }
        None => {}
    }

    match slide.note_storage {
        Some(_) => {
            document.set_presenter_notes(slide.identifier, entry.notes)?;
        }
        // A note storage is part of the slide as the theme builds it; nothing
        // here can add one, so the notes stay in this file.
        None => println!("slide {number}: no presenter-notes storage — the notes stay unwritten"),
    }

    println!("slide {number}: {}", entry.title);
    Ok(())
}

fn storage_of(placeholder: Option<&iwork::Placeholder>) -> Option<u64> {
    placeholder.and_then(|p| p.storage)
}

fn body_of(slide: &Slide) -> Option<u64> {
    storage_of(slide.body.as_ref())
}
