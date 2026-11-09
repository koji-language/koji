//! Writes std/koji/unicode.kj, the tables of Unicode of the library written in Koji, from the
//! methods of `char` of Rust (so that the methods of strings give what they gave when the runtime
//! was in Rust):
//!
//!     rustc -O bootstrap/unicode.rs -o build/unicode && build/unicode > std/koji/unicode.kj
//!
//! The tables are texts of hexadecimal digits in records of a fixed width, which a binary search
//! reads in place: a program that does not use them pays nothing to start.

/// A table of ranges of code points: `start` and `end` (inclusive), 6 digits each.
fn ranges(test: impl Fn(char) -> bool) -> String {
    let mut out = String::new();
    let mut start: Option<u32> = None;
    let mut last = 0;
    for code in 0..=0x10FFFFu32 {
        let yes = char::from_u32(code).map_or(false, &test);
        if yes {
            if start.is_none() {
                start = Some(code);
            }
            last = code;
        } else if let Some(first) = start.take() {
            out += &format!("{:06x}{:06x}", first, last);
        }
    }
    if let Some(first) = start {
        out += &format!("{:06x}{:06x}", first, last);
    }
    out
}

/// The mappings of one character to one other: runs of `start` and `end` (6 digits each), a stride
/// (1 digit: 1 or 2) and the character of `start` (6 digits); the others follow it.
fn single(map: impl Fn(char) -> Vec<char>) -> String {
    // (start, end, stride, target)
    let mut runs: Vec<(u32, u32, u32, u32)> = Vec::new();
    // The last character mapped to several: a run does not pass over it (the runs and the
    // multiple mappings do not overlap).
    let mut barrier = None;
    for code in 0..=0x10FFFFu32 {
        let Some(c) = char::from_u32(code) else { continue };
        let mapped = map(c);
        if mapped.len() > 1 {
            barrier = Some(code);
        }
        if mapped.len() != 1 || mapped[0] == c {
            continue;
        }
        let target = mapped[0] as u32;
        if let Some(run) = runs.last_mut().filter(|run| barrier.map_or(true, |b| b < run.1)) {
            let (start, end, stride, first) = *run;
            let fits = |s: u32| code == end + s && target as i64 - code as i64 == first as i64 - start as i64;
            if (start == end && (fits(1) || fits(2))) || (start != end && fits(stride)) {
                let stride = if start == end { code - end } else { stride };
                *run = (start, code, stride, first);
                continue;
            }
        }
        runs.push((code, code, 1, target));
    }
    runs.iter().map(|(start, end, stride, target)| format!("{:06x}{:06x}{}{:06x}", start, end, stride, target)).collect()
}

/// The mappings of one character to two or three: the character (6 digits), their number (1 digit)
/// and three characters (6 digits each, 0 after the last).
fn multiple(map: impl Fn(char) -> Vec<char>) -> String {
    let mut out = String::new();
    for code in 0..=0x10FFFFu32 {
        let Some(c) = char::from_u32(code) else { continue };
        let mapped = map(c);
        if mapped.len() < 2 {
            continue;
        }
        assert!(mapped.len() <= 3);
        out += &format!("{:06x}{}", code, mapped.len());
        for i in 0..3 {
            out += &format!("{:06x}", mapped.get(i).map_or(0, |&m| m as u32));
        }
    }
    out
}

/// Whether `Σ` in `before`, then `Σ`, then `after` is a final sigma in the lower case of the text.
fn final_sigma(before: &str, after: &str) -> bool {
    format!("{}Σ{}", before, after).to_lowercase() == format!("{}ς{}", before.to_lowercase(), after.to_lowercase())
}

fn main() {
    let (major, minor, update) = char::UNICODE_VERSION;
    // The two properties of the final sigma of `lower` (`Σ` at the end of a word is `ς`), which
    // `char` does not give: what `str::to_lowercase` does with them shows them. "AcΣ" ends with ς
    // when `c` is cased or ignorable, "cΣ" when it is cased and not ignorable, "AΣc" when it is
    // ignorable or not cased. Only whether a character that is not ignorable is cased matters.
    let cased = |c: char| final_sigma(&c.to_string(), "");
    let ignorable = |c: char| !cased(c) && final_sigma(&format!("A{}", c), "") && final_sigma("A", &c.to_string());
    let lower = |c: char| c.to_lowercase().collect::<Vec<char>>();
    let upper = |c: char| c.to_uppercase().collect::<Vec<char>>();
    let tables = [
        ("alphabetic", "Alphabetic", ranges(char::is_alphabetic)),
        ("numeric", "Numeric (the general categories Nd, Nl and No)", ranges(char::is_numeric)),
        ("lowercase", "Lowercase", ranges(char::is_lowercase)),
        ("uppercase", "Uppercase", ranges(char::is_uppercase)),
        ("cased", "Cased, for the final sigma (when not ignorable)", ranges(cased)),
        ("ignorable", "Case_Ignorable, for the final sigma", ranges(ignorable)),
        ("lowerSingle", "The lower case of a character, when it is one character", single(lower)),
        ("lowerMultiple", "The lower case of a character, when it is two or three", multiple(lower)),
        ("upperSingle", "The upper case of a character, when it is one character", single(upper)),
        ("upperMultiple", "The upper case of a character, when it is two or three", multiple(upper)),
    ];
    println!("// The tables of Unicode {}.{}.{} of the library written in Koji, as Rust has them: written by", major, minor, update);
    println!("// bootstrap/unicode.rs (do not edit). Texts of hexadecimal digits in records of a fixed width");
    println!("// (see std/koji/text.kj).");
    for (name, description, table) in tables {
        println!();
        println!("// {}.", description);
        println!("export immut {} = '{}'", name, table);
    }
}
