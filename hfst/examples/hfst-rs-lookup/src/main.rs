use std::io::{self, BufRead};
use std::time::Instant;

use clap::Parser;
use itertools::Itertools;

use hfst::HfstInputStream;

/// Simple version of hfst-lookup, written in Rust
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the .hfstol file
    hfst: std::path::PathBuf,

    /// Be verbose with timings
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
}

fn main() -> Result<(), String> {
    let Args { hfst, verbose } = Args::parse();

    let t0 = Instant::now();
    let Ok(is) = HfstInputStream::new(&hfst) else {
        return Err(format!("can't read hfst from file '{}'", hfst.display()));
    };

    let transducers = is.read_transducers();
    if verbose {
        println!("loaded in {:?}", Instant::now().duration_since(t0));
    }

    let Some(transducer) = transducers.first() else {
        return Err("expected at least 1 transducer in hfst".to_string());
    };

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            return Err("can't read line from stdin".to_string());
        };
        let t0 = Instant::now();
        let mut n = 0;
        for (s, w) in transducer.lookup(&line) {
            let without_ats = remove_ats(&s);
            println!("{line} → {without_ats} {w}");
            n += 1;
        }
        if n == 0 {
            println!("{line} - <not found>");
        }
        if verbose {
            println!("query took: {:?}", t0.elapsed());
        }
    }

    Ok(())
}

fn remove_ats(s: &str) -> String {
    let at_positions = s
        .char_indices()
        .filter_map(|(pos, ch)| (ch == '@').then_some(pos as i64));

    std::iter::once(-1i64)
        .chain(at_positions)
        .chain(std::iter::once(s.len() as i64))
        .tuples()
        .fold(String::new(), |mut acc, (a, b)| {
            let a = (a + 1) as usize;
            acc.push_str(&s[a..b as usize]);
            acc
        })
}
