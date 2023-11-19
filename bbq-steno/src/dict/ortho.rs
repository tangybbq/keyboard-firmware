//! English orthography rules.

// These are taken directly from the orthography rules in plover. The regexes
// have been converted to work with safe-regex which compiles the regexes to a
// state machine at compile time.

#![allow(dead_code)]

#[cfg(any())]
mod engine {
    extern crate alloc;
    use alloc::{string::String, format, vec::Vec};

    use safe_regex::regex;

    /// Pattern code for patterns that have 1 captured pattern.
    macro_rules! pat1 {
        ($src: expr, $re: expr, $replacement:expr) => {
            if let Some((a,)) =
                $re.match_slices($src.as_bytes())
            {
                return replace(&[a], $replacement);
            }
        };
    }

    /// Pattern code for patterns that have 2 captured patterns.
    macro_rules! pat2 {
        ($src: expr, $re: expr, $replacement:expr) => {
            if let Some((a, b)) =
                $re.match_slices($src.as_bytes())
            {
                return replace(&[a, b], $replacement);
            }
        };
    }

    /// Pattern code for patterns that have 2 captured patterns.
    macro_rules! pat3 {
        ($src: expr, $re: expr, $replacement:expr) => {
            if let Some((a, b, c)) =
                $re.match_slices($src.as_bytes())
            {
                return replace(&[a, b, c], $replacement);
            }
        };
    }

    fn replace(caps: &[&[u8]], replacement: &str) -> String {
        let caps: Vec<_> = caps.into_iter().map(|s| String::from_utf8(s.to_vec()).unwrap()).collect();
        let mut dollar = false;
        let mut result = String::new();
        for ch in replacement.chars() {
            if dollar {
                match ch {
                    '1' ..= '9' => {
                        let offset = (ch as usize) - ('1' as usize);
                        result.push_str(&caps[offset])
                    }
                    '$' => {
                        result.push('$');
                    }
                    _ => panic!("Invalid escape char: {:?}", ch),
                }
                dollar = false;
            } else {
                if ch == '$' {
                    dollar = true;
                } else {
                    result.push(ch)
                }
            }
        }
        result
    }

    pub fn combine(left: &str, right: &str) -> String {
        let text = format!("{} ^ {}", left, right);

        // These are taken directly from the plover english orthography rules. The
        // changes are: 1. Make the regex's br"" strings, 2. change the replacement
        // to an r"", 3. Replace the backslashes in the replacement with dollar (not
        // really needed, but I'd already done it for regex. 4. Insert into macro to
        // handle. 5. Remove the '^' and '$' from the patterns (these don't work
        // with safe regex, the patterns are always full matches.

        // == +ly ==
        // artistic + ly = artistically
        pat1!(text, regex!(br"(.*[aeiou]c) \^ ly"), r"$1ally");
        // humble + ly = humbly (*humblely)
        // questionable +ly = questionably
        // triple +ly = triply
        pat1!(text, regex!(br"(.+[aeioubmnp])le \^ ly"), r"$1ly");

        // == +ry ==
        // statute + ry = statutory
        pat2!(text, regex!(br"(.*t)e \^ (ry|ary)"), r"$1ory");
        // confirm +tory = confirmatory (*confirmtory)
        pat2!(text, regex!(br"(.+)m \^ tor(y|ily)"), r"$1mator$2");
        // supervise +ary = supervisory (*supervisary)
        pat2!(text, regex!(br"(.+)se \^ ar(y|ies)"), r"$1sor$2");

        // == t +cy ==
        // frequent + cy = frequency (tcy/tecy removal)
        pat1!(text, regex!(br"(.*[naeiou])te? \^ cy"), r"$1cy");

        // == +s ==
        // establish + s = establishes (sibilant pluralization)
        pat1!(text, regex!(br"(.*(?:s|sh|x|z|zh)) \^ s"), r"$1es");

        // speech + s = speeches (soft ch pluralization)
        // PERL crap: TODO: Can we do this without this?
        // (r"^(.*(?:oa|ea|i|ee|oo|au|ou|l|n|(?<![gin]a)r|t)ch) \^ s$", r"$1es"),
        pat1!(text, regex!(br"(.*(?:oa|ea|i|ee|oo|au|ou|l|n|[^gin]ar|t)ch) \^ s"), r"$1es");

        // cherry + s = cherries (consonant + y pluralization)
        pat1!(text, regex!(br"(.+[bcdfghjklmnpqrstvwxz])y \^ s"), r"$1ies");

        // == y ==
        // die+ing = dying
        pat1!(text, regex!(br"(.+)ie \^ ing"), r"$1ying");
        // metallurgy + ist = metallurgist
        pat1!(text, regex!(br"(.+[cdfghlmnpr])y \^ ist"), r"$1ist");
        // beauty + ful = beautiful (y -> i)
        pat2!(text, regex!(br"(.+[bcdfghjklmnpqrstvwxz])y \^ ([a-hj-xz].*)"), r"$1i$2");

        // == +en ==
        // write + en = written
        pat1!(text, regex!(br"(.+)te \^ en"), r"$1tten");
        // Minessota +en = Minessotan (*Minessotaen)
        pat2!(text, regex!(br"(.+[ae]) \^ e(n|ns)"), r"$1$2");

        // == +ial ==
        // ceremony +ial = ceremonial (*ceremonyial)
        pat2!(text, regex!(br"(.+)y \^ (ial|ially)"), r"$1$2");
        // == +if ==
        // spaghetti +ification = spaghettification (*spaghettiification)
        pat2!(text, regex!(br"(.+)i \^ if(y|ying|ied|ies|ication|ications)"), r"$1if$2");

        // == +ical ==
        // fantastic +ical = fantastical (*fantasticcal)
        pat2!(text, regex!(br"(.+)ic \^ (ical|ically)"), r"$1$2");
        // epistomology +ical = epistomological
        pat2!(text, regex!(br"(.+)ology \^ ic(al|ally)"), r"$1ologic$2");
        // oratory +ical = oratorical (*oratoryical)
        pat2!(text, regex!(br"(.*)ry \^ ica(l|lly|lity)"), r"$1rica$2");

        // == +ist ==
        // radical +ist = radicalist (*radicallist)
        pat2!(text, regex!(br"(.*[l]) \^ is(t|ts)"), r"$1is$2");

        // == +ity ==
        // complementary +ity = complementarity (*complementaryity)
        pat1!(text, regex!(br"(.*)ry \^ ity"), r"$1rity");
        // disproportional +ity = disproportionality (*disproportionallity)
        pat1!(text, regex!(br"(.*)l \^ ity"), r"$1lity");

        // == +ive, +tive ==
        // perform +tive = performative (*performtive)
        pat2!(text, regex!(br"(.+)rm \^ tiv(e|ity|ities)"), r"$1rmativ$2");
        // restore +tive = restorative
        pat2!(text, regex!(br"(.+)e \^ tiv(e|ity|ities)"), r"$1ativ$2");

        // == +ize ==
        // token +ize = tokenize (*tokennize)
        // token +ise = tokenise (*tokennise)
        pat2!(text, regex!(br"(.+)y \^ iz(e|es|ing|ed|er|ers|ation|ations|able|ability)"), r"$1iz$2");
        pat2!(text, regex!(br"(.+)y \^ is(e|es|ing|ed|er|ers|ation|ations|able|ability)"), r"$1is$2");
        // conditional +ize = conditionalize (*conditionallize)
        pat2!(text, regex!(br"(.+)al \^ iz(e|ed|es|ing|er|ers|ation|ations|m|ms|able|ability|abilities)"), r"$1aliz$2");
        pat2!(text, regex!(br"(.+)al \^ is(e|ed|es|ing|er|ers|ation|ations|m|ms|able|ability|abilities)"), r"$1alis$2");
        // spectacular +ization = spectacularization (*spectacularrization)
        pat2!(text, regex!(br"(.+)ar \^ iz(e|ed|es|ing|er|ers|ation|ations|m|ms)"), r"$1ariz$2");
        pat2!(text, regex!(br"(.+)ar \^ is(e|ed|es|ing|er|ers|ation|ations|m|ms)"), r"$1aris$2");

        // category +ize/+ise = categorize/categorise (*categoryize/ *categoryise)
        // custom +izable/+isable = customizable/customisable (*custommizable/ *custommisable)
        // fantasy +ize = fantasize (*fantasyize)
        pat2!(text, regex!(br"(.*[lmnty]) \^ iz(e|es|ing|ed|er|ers|ation|ations|m|ms|able|ability|abilities)"), r"$1iz$2");
        pat2!(text, regex!(br"(.*[lmnty]) \^ is(e|es|ing|ed|er|ers|ation|ations|m|ms|able|ability|abilities)"), r"$1is$2");

        // == +olog ==
        // criminal + ology = criminology
        // criminal + ologist = criminalogist (*criminallologist)
        pat2!(text, regex!(br"(.+)al \^ olog(y|ist|ists|ical|ically)"), r"$1olog$2");

        // == +ish ==
        // similar +ish = similarish (*similarrish)
        pat2!(text, regex!(br"(.+)(ar|er|or) \^ ish"), r"$1$2ish");

        // free + ed = freed
        pat2!(text, regex!(br"(.+e)e \^ (e.+)"), r"$1$2");
        // narrate + ing = narrating (silent e)
        pat2!(text, regex!(br"(.+[bcdfghjklmnpqrstuvwxz])e \^ ([aeiouy].*)"), r"$1$2");

        // == misc ==
        // defer + ed = deferred (consonant doubling)   XXX monitor(stress not on last syllable)
        pat3!(text, regex!(br"(.*(?:[bcdfghjklmnprstvwxyz]|qu)[aeiou])([bcdfgklmnprtvz]) \^ ([aeiouy].*)"), r"$1$2$2$3");

        format!("{}{}", left, right)
    }
}

#[cfg(all())]
mod engine {
    extern crate alloc;
    use alloc::{string::String, format};

    pub fn combine(left: &str, right: &str) -> String {
        format!("{}{}", left, right)
    }
}

pub use engine::combine;
