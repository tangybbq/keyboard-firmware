//! Drill sheets for learning Orsy, one lesson at a time.
//!
//! Prints `docs/orsy/drills.md`, a lesson-by-lesson list of words with their
//! strokes, and `docs/orsy/drills/NN-name.txt`, one line of words per lesson
//! for pasting into MonkeyType.  The lessons follow the order in
//! `docs/orsy/04-training.md`: what transfers from Dosh, then the shapes that
//! conflict with Dosh one at a time, then the vowels and second characters,
//! the free shapes, and the composition rules.
//!
//! A word belongs to the first lesson that lets it be written in as few
//! strokes as it ever takes, the rule `words/drills.py` uses for the n-gram
//! chords.  So a word appears once, where the last thing it needs is taught,
//! and a lesson's list is exactly the words it makes writable.
//!
//!     cargo run --release --example lessons [--words FILE] [--out DIR] [--limit N]

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use bbq_orsy::tables::{Outer, Second, Vowel};
use bbq_orsy::{rules, translate, Chord, Patterns, Writer};

/// Something a lesson teaches.
#[derive(Clone, Copy)]
enum Item {
    Outer(Outer),
    Second(Second),
    Vowel(Vowel),
    Rule(u8),
}

struct Lesson {
    /// The file name stem.
    name: &'static str,
    title: &'static str,
    /// What is new, and what the Dosh hand will want to do instead.
    note: &'static str,
    items: &'static [Item],
}

use Item::*;

const LESSONS: &[Lesson] = &[
    Lesson {
        name: "transfer",
        title: "what transfers from Dosh",
        note: "`s`, `t` and `n` on the outer keys, and `e` and `i` on the inner, are \
               Dosh's own keys and mean the same here.  Add `Bk`, the space thumb, to a \
               vowel to end the word.  Also the two commands with nothing to learn: right \
               `Bk` alone is a space, and the whole left outer row is undo.",
        items: &[
            Outer(Outer::S), Outer(Outer::FP), Outer(Outer::N),
            Vowel(Vowel::E), Vowel(Vowel::Ue), Vowel(Vowel::I), Vowel(Vowel::Ui),
        ],
    },
    Lesson { name: "r", title: "r on the pinky", note: "Dosh types `a` here.", items: &[Outer(Outer::FCN)] },
    Lesson { name: "c", title: "c on the lower ring", note: "Dosh types `o` here.", items: &[Outer(Outer::CP)] },
    Lesson { name: "d", title: "d is t plus the pinky", note: "Dosh types `q` here.", items: &[Outer(Outer::SCP)] },
    Lesson { name: "p", title: "p on the ring pair", note: "Dosh has one-shot alt here.", items: &[Outer(Outer::P)] },
    Lesson { name: "f", title: "f on lower ring and middle", note: "Dosh types `u` here.", items: &[Outer(Outer::F)] },
    Lesson { name: "y", title: "y on pinky and upper middle", note: "Dosh types `j` here.", items: &[Outer(Outer::ZN)] },
    Lesson { name: "th", title: "th on lower ring and upper middle", note: "Dosh types `g` here.", items: &[Outer(Outer::FZ)] },
    Lesson { name: "l", title: "l on upper ring and lower middle", note: "Dosh types `?` here.", items: &[Outer(Outer::SCN)] },
    Lesson { name: "b", title: "b is p plus the pinky", note: "Dosh has one-shot gui here.", items: &[Outer(Outer::FCP)] },
    Lesson { name: "w", title: "w on the upper ring and middle", note: "Dosh types `p` here.", items: &[Outer(Outer::CN)] },
    Lesson { name: "g", title: "g is c plus the pinky", note: "Dosh types `l` here.", items: &[Outer(Outer::ZP)] },
    Lesson {
        name: "h-st", title: "h as an onset, st as a coda",
        note: "The one shape that reads differently on the two hands.  Dosh has one-shot control here.",
        items: &[Outer(Outer::FC)],
    },
    Lesson { name: "x", title: "x on the middle pair and lower ring", note: "Dosh has PrintScreen here.", items: &[Outer(Outer::SZN)] },
    Lesson {
        name: "a-o", title: "the vowels a and o",
        note: "`a` is the `Sp` thumb alone, which is Backspace in Dosh; `o` is `e+i`, \
               which is Dosh's one-shot shift.  Plus `Bk` to end the word.",
        items: &[Vowel(Vowel::A), Vowel(Vowel::Ua), Vowel(Vowel::Ie), Vowel(Vowel::Uie)],
    },
    Lesson {
        name: "u", title: "the vowel u",
        note: "`i+Sp`, which is Dosh's full stop.",
        items: &[Vowel(Vowel::U), Vowel(Vowel::Uia)],
    },
    Lesson {
        name: "second-r-l", title: "the second character: r and l",
        note: "The left hand's inner four are the second character of the syllable, \
               mostly consonants.  `r` is `Bk` alone and `l` is `Sp` alone.",
        items: &[Second(Second::R), Second(Second::RI)],
    },
    Lesson {
        name: "second-i-t-m", title: "the second character: i, t and m",
        note: "`i` is the same key as the vowel; `t` is `e+Sp`, `m` is `e+Bk`.",
        items: &[Second(Second::I), Second(Second::RIU), Second(Second::RU)],
    },
    Lesson {
        name: "second-o-s-e", title: "the second character: o, s and e",
        note: "`o` is `e+i` as for the vowel; `s` is `e` alone; `e` is `i+Bk`.",
        items: &[Second(Second::RXI), Second(Second::X), Second(Second::RX)],
    },
    Lesson {
        name: "second-rest", title: "the second character: w, c, u, p and n",
        note: "`w` is `Sp+Bk`, `c` is `e+i+Sp`, `u` is `i+Sp` as for the vowel, `p` is \
               `e+i+Bk`, `n` is `e+Sp+Bk`.",
        items: &[Second(Second::XI), Second(Second::XIU), Second(Second::U), Second(Second::IU), Second(Second::XU)],
    },
    Lesson {
        name: "ea-ou", title: "ea and ou",
        note: "`ea` is `e+Sp` (e plus a); `ou` is `e+i+Sp+Bk` (o plus u), and only ever \
               ends a word.  These are the free combinations: they spell the digraph only \
               with a second character, which is why they come after those; `ea` \
               alone is a placeholder glyph.",
        items: &[Vowel(Vowel::Ea), Vowel(Vowel::Iea), Vowel(Vowel::Ia), Rule(rules::FREE)],
    },
    Lesson {
        name: "v-m-k", title: "v, m and k",
        note: "Free in Dosh: nothing to unlearn.  `v` is f plus the pinky.",
        items: &[Outer(Outer::SC), Outer(Outer::SZP), Outer(Outer::SZ)],
    },
    Lesson {
        name: "nd-ng-nt", title: "ind/nd, inc/ng and int/nt",
        note: "Three-key shapes that spell a whole cluster: `ind`, `inc` and `int` as \
               onsets, `nd`, `ng` and `nt` as codas.",
        items: &[Outer(Outer::FN), Outer(Outer::SN), Outer(Outer::FZN)],
    },
    Lesson {
        name: "ch-sh-gh-z-ck", title: "ch, sh, gh, z and ck",
        note: "`sh` is ch plus the pinky; `z` is s plus the pinky.",
        items: &[Outer(Outer::SP), Outer(Outer::C), Outer(Outer::FZP), Outer(Outer::Z), Outer(Outer::CZ)],
    },
    Lesson {
        name: "coda-h-e", title: "the coda-only shapes: h and e",
        note: "Two four-key right-hand shapes with no onset reading: a final `h` and a \
               final `e`.",
        items: &[Outer(Outer::FCZ), Outer(Outer::CZP)],
    },
    Lesson {
        name: "silent-e", title: "the mirrored vowel and the silent e",
        note: "With no vowel on the right hand and a real coda, a second character that \
               is also a vowel supplies the nucleus, and the stroke appends a silent `e` \
               and ends the word: `time`, `tone`, `tame`.",
        items: &[Rule(rules::MIRRORED)],
    },
    Lesson {
        name: "clusters", title: "the onset clusters",
        note: "An onset and a second character that spell a cluster together: `h+r` is \
               `str`, `h+l` is `spl`, `h+p` is `spr`, `h+c` is `scr`, `sh+c` is `sch`, \
               `z+c` is `sk`, `s+s` is `sci`, `y+i` is `j`, `c+c` is `qu`.",
        items: &[Rule(rules::CLUSTER)],
    },
    Lesson {
        name: "h-after-p-w-r", title: "w reads as h after p, w and r",
        note: "The second character `w` (`Sp+Bk`) spells `h` after an onset ending in \
               `p`, `w` or `r`: `ph`, `wh`, `rh`.",
        items: &[Rule(rules::XI_H)],
    },
    Lesson {
        name: "diphthongs", title: "au and ai",
        note: "A second-character `u` before the vowel `u` spells `au`, and `i` before \
               `i` spells `ai`.",
        items: &[Rule(rules::DIPHTHONG)],
    },
    Lesson {
        name: "final-y", title: "a bare final y",
        note: "`y` as a coda with the `i`-ending vowel keys spells just `y` and ends the \
               word: the `ui` keys give the space and nothing else.",
        items: &[Rule(rules::BARE_Y)],
    },
];

/// The patterns and rules taught so far.
#[derive(Default, Clone)]
struct Allowed {
    outer: u32,
    second: u16,
    vowel: u16,
    rules: u8,
}

fn index<T: PartialEq>(all: &[T], item: T) -> usize {
    all.iter().position(|x| *x == item).expect("pattern in table")
}

impl Allowed {
    fn add(&mut self, item: Item) {
        match item {
            Outer(o) => self.outer |= 1 << index(&Outer::ALL, o),
            Second(s) => self.second |= 1 << index(&Second::ALL, s),
            Vowel(v) => self.vowel |= 1 << index(&Vowel::ALL, v),
            Rule(r) => self.rules |= r,
        }
    }

    fn has_outer(&self, o: Outer) -> bool {
        o == Outer::Empty || self.outer & (1 << index(&Outer::ALL, o)) != 0
    }

    fn accepts(&self, chord: Chord, used: u8) -> bool {
        let Some(p) = Patterns::of(chord) else { return false };
        self.has_outer(p.onset)
            && self.has_outer(p.coda)
            && (p.second == Second::Empty || self.second & (1 << index(&Second::ALL, p.second)) != 0)
            && (p.vowel == Vowel::Empty || self.vowel & (1 << index(&Vowel::ALL, p.vowel)) != 0)
            && used & !self.rules == 0
    }
}

/// A hand's keys, in the order the documents write them.
fn keys(hand: u16) -> String {
    const NAMES: [(u16, &str); 9] = [
        (0x001, "a"), (0x002, "o"), (0x020, "s"), (0x004, "t"), (0x040, "n"),
        (0x008, "e"), (0x080, "i"), (0x100, "Sp"), (0x200, "Bk"),
    ];
    NAMES.iter().filter(|(bit, _)| hand & bit != 0).map(|(_, n)| *n).collect()
}

fn stroke(chord: Chord) -> String {
    format!("{}-{}", keys(chord.left), keys(chord.right))
}

struct Entry {
    word: String,
    strokes: Vec<Chord>,
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut words_path = root.join("words/english_10k.json");
    let mut out = root.join("docs/orsy");
    let mut limit = 40usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().expect("value");
        match arg.as_str() {
            "--words" => words_path = PathBuf::from(value()),
            "--out" => out = PathBuf::from(value()),
            "--limit" => limit = value().parse().expect("number"),
            other => panic!("unknown argument {other}"),
        }
    }

    let list: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&words_path).expect("word list")).expect("json");
    let mut seen = HashSet::new();
    let words: Vec<String> = list["words"]
        .as_array()
        .expect("words array")
        .iter()
        .filter_map(|w| w.as_str())
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| w.bytes().all(|b| b.is_ascii_lowercase()) && seen.insert(w.clone()))
        .collect();

    let writer = Writer::new();

    // The cumulative set after each lesson.
    let mut allowed = Vec::new();
    let mut so_far = Allowed::default();
    for lesson in LESSONS {
        for item in lesson.items {
            so_far.add(*item);
        }
        allowed.push(so_far.clone());
    }

    let mut lessons: Vec<Vec<Entry>> = (0..LESSONS.len()).map(|_| Vec::new()).collect();
    let mut unwritable = Vec::new();
    let mut never = Vec::new();
    for word in &words {
        let Some(best) = writer.write(word) else {
            unwritable.push(word.clone());
            continue;
        };
        let placed = allowed.iter().enumerate().find_map(|(i, a)| {
            let strokes = writer.write_with(word, |c, r| a.accepts(c, r))?;
            (strokes.len() == best.len()).then_some((i, strokes))
        });
        match placed {
            Some((i, strokes)) => lessons[i].push(Entry { word: word.clone(), strokes }),
            None => never.push(word.clone()),
        }
    }

    // The markdown.
    let mut md = String::new();
    writeln!(md, "# Orsy drills\n").unwrap();
    writeln!(
        md,
        "Generated by `cargo run --release --example lessons` in `bbq-orsy`; do not edit. \
         The lessons follow `04-training.md`.  A word is listed once, in the lesson that \
         completes it: the first lesson after which it can be written in as few strokes \
         as it ever takes.  Words are in frequency order, from MonkeyType's `english_10k`, \
         and each lesson's `drills/` file is the same words on one line for pasting into \
         MonkeyType.\n"
    )
    .unwrap();
    writeln!(
        md,
        "Strokes are written left hand, hyphen, right hand, in the keys' own names \
         (`a o s t n` outer, `e i Sp Bk` inner), so `at-eBk` is `a`+`t` on the left and \
         `e`+`Bk` on the right.  The division column shows what each stroke spells.\n"
    )
    .unwrap();
    writeln!(md, "| lesson | new | words |").unwrap();
    writeln!(md, "|---|---|---|").unwrap();
    for (i, lesson) in LESSONS.iter().enumerate() {
        writeln!(md, "| {} | {} | {} |", i + 1, lesson.title, lessons[i].len()).unwrap();
    }
    writeln!(md).unwrap();
    for (i, lesson) in LESSONS.iter().enumerate() {
        writeln!(md, "## Lesson {} — {}\n", i + 1, lesson.title).unwrap();
        writeln!(md, "{}\n", lesson.note).unwrap();
        let entries = &lessons[i];
        writeln!(
            md,
            "{} words qualify, {} here, most common first.  `drills/{:02}-{}.txt`.\n",
            entries.len(),
            entries.len().min(limit),
            i + 1,
            lesson.name
        )
        .unwrap();
        if entries.is_empty() {
            continue;
        }
        writeln!(md, "| word | strokes | division |").unwrap();
        writeln!(md, "|---|---|---|").unwrap();
        for e in entries.iter().take(limit) {
            let strokes: Vec<String> = e.strokes.iter().map(|c| stroke(*c)).collect();
            let division: Vec<String> =
                e.strokes.iter().map(|c| translate(*c).unwrap().text()).collect();
            writeln!(md, "| {} | `{}` | {} |", e.word, strokes.join(" "), division.join("·"))
                .unwrap();
        }
        writeln!(md).unwrap();
    }
    if !unwritable.is_empty() {
        writeln!(md, "## Not writable\n").unwrap();
        writeln!(
            md,
            "{} words of the list cannot be written by the rules at all, and need the \
             Dosh escape: {}\n",
            unwritable.len(),
            unwritable.join(", ")
        )
        .unwrap();
    }
    fs::write(out.join("drills.md"), md).expect("write drills.md");

    // The MonkeyType files, one line each, no trailing newline.
    let dir = out.join("drills");
    fs::create_dir_all(&dir).expect("drills dir");
    for stale in fs::read_dir(&dir).expect("drills dir").flatten() {
        let name = stale.file_name().to_string_lossy().to_string();
        if name.ends_with(".txt") && name.chars().take(2).all(|c| c.is_ascii_digit()) {
            fs::remove_file(stale.path()).expect("remove stale drill");
        }
    }
    for (i, lesson) in LESSONS.iter().enumerate() {
        let words: Vec<&str> = lessons[i].iter().take(limit).map(|e| e.word.as_str()).collect();
        let path = dir.join(format!("{:02}-{}.txt", i + 1, lesson.name));
        fs::write(&path, words.join(" ")).expect("write drill");
        eprintln!("{:26} {:5} words ({} in file)", path.file_name().unwrap().to_string_lossy(), lessons[i].len(), words.len());
    }
    eprintln!("{} words; {} unwritable; {} never placed", words.len(), unwritable.len(), never.len());
    if !never.is_empty() {
        eprintln!("never placed: {}", never.join(" "));
    }
}
