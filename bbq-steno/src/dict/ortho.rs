//! English orthography rules.

extern crate alloc;
use alloc::{string::{String, ToString}, vec::Vec, format};
use regex_lite::Regex;

pub struct Ortho {
    rules: Vec<(Regex, String)>,
}

impl Ortho {
    pub fn new() -> Result<Ortho, ()> {
        let rules = ORTHO_BASE.iter().map(|(a, b)| {
            let r2 = Regex::new(a).map_err(|_| ())?;
            Ok((r2, b.to_string()))
        });
        let rules: Result<Vec<_>, ()> = rules.collect();
        let rules = rules?;
        Ok(Ortho { rules })
    }

    pub fn combine(&self, left: &str, right: &str) -> String {
        let start = format!("{} ^ {}", left, right);
        for (rule, replacement) in &self.rules {
            if let Some(cap) = rule.captures(&start) {
                let mut result = String::new();
                cap.expand(replacement, &mut result);
                return result;
            }
        }

        // If no rules match, just combine them.
        format!("{}{}", left, right)
    }
}

/// Othography rules copied out of plover, and changed to Rust string syntax.
static ORTHO_BASE: &[(&str, &str)] = &[
    // == +ly ==
    // artistic + ly = artistically
    (r"^(.*[aeiou]c) \^ ly$", r"$1ally"),
    // humble + ly = humbly (*humblely)
    // questionable +ly = questionably
    // triple +ly = triply
    (r"^(.+[aeioubmnp])le \^ ly$", r"$1ly"),

    // == +ry ==
    // statute + ry = statutory
    (r"^(.*t)e \^ (ry|ary)$", r"$1ory"),
    // confirm +tory = confirmatory (*confirmtory)
    (r"^(.+)m \^ tor(y|ily)$", r"$1mator$2"),
    // supervise +ary = supervisory (*supervisary)
    (r"^(.+)se \^ ar(y|ies)$", r"$1sor$2"),

    // == t +cy ==
    // frequent + cy = frequency (tcy/tecy removal)
    (r"^(.*[naeiou])te? \^ cy$", r"$1cy"),

    // == +s ==
    // establish + s = establishes (sibilant pluralization)
    (r"^(.*(?:s|sh|x|z|zh)) \^ s$", r"$1es"),

    // speech + s = speeches (soft ch pluralization)
    // PERL crap: TODO: Can we do this without this?
    // (r"^(.*(?:oa|ea|i|ee|oo|au|ou|l|n|(?<![gin]a)r|t)ch) \^ s$", r"$1es"),
    (r"^(.*(?:oa|ea|i|ee|oo|au|ou|l|n|[^gin]ar|t)ch) \^ s$", r"$1es"),

    // cherry + s = cherries (consonant + y pluralization)
    (r"^(.+[bcdfghjklmnpqrstvwxz])y \^ s$", r"$1ies"),

    // == y ==
    // die+ing = dying
    (r"^(.+)ie \^ ing$", r"$1ying"),
    // metallurgy + ist = metallurgist
    (r"^(.+[cdfghlmnpr])y \^ ist$", r"$1ist"),
    // beauty + ful = beautiful (y -> i)
    (r"^(.+[bcdfghjklmnpqrstvwxz])y \^ ([a-hj-xz].*)$", r"$1i$2"),

    // == +en ==
    // write + en = written
    (r"^(.+)te \^ en$", r"$1tten"),
    // Minessota +en = Minessotan (*Minessotaen)
    (r"^(.+[ae]) \^ e(n|ns)$", r"$1$2"),

    // == +ial ==
    // ceremony +ial = ceremonial (*ceremonyial)
    (r"^(.+)y \^ (ial|ially)$", r"$1$2"),
    // == +if ==
    // spaghetti +ification = spaghettification (*spaghettiification)
    (r"^(.+)i \^ if(y|ying|ied|ies|ication|ications)$", r"$1if$2"),

    // == +ical ==
    // fantastic +ical = fantastical (*fantasticcal)
    (r"^(.+)ic \^ (ical|ically)$", r"$1$2"),
    // epistomology +ical = epistomological
    (r"^(.+)ology \^ ic(al|ally)$", r"$1ologic$2"),
    // oratory +ical = oratorical (*oratoryical)
    (r"^(.*)ry \^ ica(l|lly|lity)$", r"$1rica$2"),

    // == +ist ==
    // radical +ist = radicalist (*radicallist)
    (r"^(.*[l]) \^ is(t|ts)$", r"$1is$2"),

    // == +ity ==
    // complementary +ity = complementarity (*complementaryity)
    (r"^(.*)ry \^ ity$", r"$1rity"),
    // disproportional +ity = disproportionality (*disproportionallity)
    (r"^(.*)l \^ ity$", r"$1lity"),

    // == +ive, +tive ==
    // perform +tive = performative (*performtive)
    (r"^(.+)rm \^ tiv(e|ity|ities)$", r"$1rmativ$2"),
    // restore +tive = restorative
    (r"^(.+)e \^ tiv(e|ity|ities)$", r"$1ativ$2"),

    // == +ize ==
    // token +ize = tokenize (*tokennize)
    // token +ise = tokenise (*tokennise)
    (r"^(.+)y \^ iz(e|es|ing|ed|er|ers|ation|ations|able|ability)$", r"$1iz$2"),
    (r"^(.+)y \^ is(e|es|ing|ed|er|ers|ation|ations|able|ability)$", r"$1is$2"),
    // conditional +ize = conditionalize (*conditionallize)
    (r"^(.+)al \^ iz(e|ed|es|ing|er|ers|ation|ations|m|ms|able|ability|abilities)$", r"$1aliz$2"),
    (r"^(.+)al \^ is(e|ed|es|ing|er|ers|ation|ations|m|ms|able|ability|abilities)$", r"$1alis$2"),
    // spectacular +ization = spectacularization (*spectacularrization)
    (r"^(.+)ar \^ iz(e|ed|es|ing|er|ers|ation|ations|m|ms)$", r"$1ariz$2"),
    (r"^(.+)ar \^ is(e|ed|es|ing|er|ers|ation|ations|m|ms)$", r"$1aris$2"),

    // category +ize/+ise = categorize/categorise (*categoryize/*categoryise)
    // custom +izable/+isable = customizable/customisable (*custommizable/*custommisable)
    // fantasy +ize = fantasize (*fantasyize)
    (r"^(.*[lmnty]) \^ iz(e|es|ing|ed|er|ers|ation|ations|m|ms|able|ability|abilities)$", r"$1iz$2"),
    (r"^(.*[lmnty]) \^ is(e|es|ing|ed|er|ers|ation|ations|m|ms|able|ability|abilities)$", r"$1is$2"),

    // == +olog ==
    // criminal + ology = criminology
    // criminal + ologist = criminalogist (*criminallologist)
    (r"^(.+)al \^ olog(y|ist|ists|ical|ically)$", r"$1olog$2"),

    // == +ish ==
    // similar +ish = similarish (*similarrish)
    (r"^(.+)(ar|er|or) \^ ish$", r"$1$2ish"),

    // free + ed = freed
    (r"^(.+e)e \^ (e.+)$", r"$1$2"),
    // narrate + ing = narrating (silent e)
    (r"^(.+[bcdfghjklmnpqrstuvwxz])e \^ ([aeiouy].*)$", r"$1$2"),

    // == misc ==
    // defer + ed = deferred (consonant doubling)   XXX monitor(stress not on last syllable)
    (r"^(.*(?:[bcdfghjklmnprstvwxyz]|qu)[aeiou])([bcdfgklmnprtvz]) \^ ([aeiouy].*)$", r"$1$2$2$3"),
];
