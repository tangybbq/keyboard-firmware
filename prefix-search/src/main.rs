fn main() {
    testquery("cat");
    testquery("catdog");
    testquery("catalogdog");
    testquery("zebra");
    testquery("zebraxxx");
}

static DICT: &[&str] = &[
    "cat", "catalog", "dog", "zebra",
];

fn testquery(query: &str) {
    println!("{} -> {:?}", query, psearch(query));
}

fn psearch(query: &str) -> Option<&str> {
    // The best result we've seen so far.
    let mut best = None;

    // How many characters of the query are we searching for.
    let mut used = 1;

    // Shortcut for our starting position.
    let mut start = 0;

    // Perform a search on the prefix we are currently looking at.
    loop {
        let subdict = &DICT[start..];
        let subentry = &query[0..used];
        match subdict.binary_search(&subentry) {
            Ok(pos) => {
                let pos = start + pos;
                // This matches, so consider it a potential candidate. Longer
                // results will replace this.
                best = Some(subentry);

                if used == query.len() {
                    break;
                }

                // Since this matches, see if we have any longer prefixes that match.
                start = pos + 1;
                used += 1;
            }
            Err(pos) => {
                let pos = start + pos;

                if used == query.len() {
                    break;
                }

                // If we are at the end of the dictionary, there is also nothing more to search for.
                if pos >= DICT.len() {
                    break;
                }

                // Nothing matches, but we are at the place this text would be
                // inserted. If this position is indeed a position that matches,
                // there are potentially matches if we accept more characters
                // from our query.
                if DICT[pos].starts_with(subentry) {
                    // Start here, since a longer query could match this entry.
                    start = pos;
                    // But search for an additional character.
                    used += 1;
                } else {
                    // There aren't any more possible matches, so return
                    // whatever best result we've seen so far.
                    break;
                }
            }
        }
    }

    return best;
}
