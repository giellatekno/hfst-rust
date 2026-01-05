//! This example serves an axum app, which expose a single route to do analysis "in
//! parallel" using the "tokio-actors" feature of "hfst" that gives us an actor we can
//! query for results from mulitple futures.

use std::sync::LazyLock;
use std::collections::HashMap;

use axum::{
    extract::Query, response::{IntoResponse, Response}, routing::get, Router
};
use serde::Deserialize;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use hfst::transducer_actor::HfstTransducerActor;
use without_ats::without_ats_iter;

// String (3-character language code) -> hfst transducer actor
type ActorMap = HashMap<String, HfstTransducerActor>;

static HFST_TRANSDUCER_ACTORS: LazyLock<ActorMap> = LazyLock::new(initialize_hfst_transducer_actors);

async fn analyze(lang: &str, text: &str) -> Result<Vec<(String, Vec<(String, f32)>)>, String> {
    let actor = HFST_TRANSDUCER_ACTORS.get(lang)
        .ok_or_else(|| format!("no transducer for language {lang}"))?;

    let inputs = text.split('\n')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let mut results = vec![];
    for input in inputs {
        let input = input.to_owned();
        let lookup_results = actor.lookup(&input).await.map_err(|e| format!("{e}"))?;
        results.push((input, lookup_results.results));
    }
    Ok(results)
}

fn read_lang_dir(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let entries = path.read_dir().expect("can read language dir");
    for entry in entries {
        let entry = entry.expect("can read directory entry");
        if entry.file_name() != "analyser-gt-desc.hfstol" {
            continue;
        }
        return Some(entry.path());
    }
    None
}

fn initialize_hfst_transducer_actors() -> ActorMap {
    let apt_nightly_dir = std::env::var("AP_NIGHTLY_DIR").unwrap_or_else(|_e| {
        String::from("/usr/share/giella")
    });

    let apt_nightly_dir = std::path::PathBuf::from(apt_nightly_dir);

    let mut actors = HashMap::new();
    for entry in apt_nightly_dir.read_dir().expect("can read AP_NIGHTLY_DIR") {
        let entry = entry.expect("can read next directory entry");
        if !entry.file_type().expect("can read file metadata").is_dir() {
            continue;
        }
        let lang = entry.file_name();
        if lang.len() != 3 {
            // skip dirs whose name is not 3 characters (not a language dir)
            continue;
        }
        let lang = lang.into_string().expect("can convert OsString to String");
        if let Some(hfst_path) = read_lang_dir(&entry.path()) {
            let transducer = hfst::HfstInputStream::new(hfst_path)
                .expect("can load HfstInputStream from file path")
                .read_only_transducer()
                .expect("hfst input stream of analyser-gt-desc.hfstol contains exactly 1 transducer");
            let actor = HfstTransducerActor::builder()
                .transducer(transducer)
                .queue_size(std::num::NonZeroUsize::new(50).unwrap())
                .build();
            actors.insert(lang, actor);
        }
    }
    actors
}

#[derive(Deserialize)]
struct QueryParams {
    lang: String,
    text: String,
}

async fn analyze_endpoint(
    Query(QueryParams { lang, text }): Query<QueryParams>,
) -> Response {
    match analyze(&lang, &text).await {
        Ok(all_replies) => {
            let mut out = String::new();

            // For all inputs (inputs are newline-delimited, and there can be more than,
            // e.g. `curl "http://localhost:3001/?lang=sme&text=viessu%0ANew%20York"
            for (input_text, replies) in all_replies {

                // For all strings found for this input...
                for (value, weight) in replies {
                    // Print which input this output belongs to, followed by TAB
                    out.push_str(&input_text);
                    out.push('\t');

                    // Print this reply. We use without_ats_iter to not get the flag
                    // diacritics
                    for substr in without_ats_iter(&value) {
                        out.push_str(substr);
                    }

                    out.push('\t');
                    out.push_str(&format!("{weight}"));
                    out.push('\n');
                }
                out.push('\n');
            }
            out.into_response()
        }
        Err(error) => format!("error: {error}\n").into_response(),
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // axum logs rejections from built-in extractors with the `axum::rejection`
                // target, at `TRACE` level. `axum::rejection=trace` enables showing those events
                format!(
                    "{}=debug,tower_http=debug,axum::rejection=trace",
                    env!("CARGO_CRATE_NAME")
                )
                .into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // build our application with a single route
    let app = Router::new().route("/", get(analyze_endpoint));

    let t0 = std::time::Instant::now();
    // Reading HFST_TRANSDUCER_ACTORS will initialize it, which will load actors for
    // all found languages.
    let num_langs = HFST_TRANSDUCER_ACTORS.len();
    let dur = t0.elapsed();
    if num_langs == 0 {
        tracing::warn!(?dur, "No languages found! This API won't be able to do anything!");
    } else {
        tracing::info!(?dur, num_langs, "Loaded langs");
    }

    // run our app with hyper, listening globally on the first available port, starting
    // from 3000 and incrementing port until we find an available one
    let mut ports = 3000..u16::MAX;
    let listener = loop {
        let Some(port) = ports.next() else {
            tracing::error!("all ports on system busy, aborting");
            return;
        };

        let addr = ("0.0.0.0", port);
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                tracing::info!(port, "listening on port");
                break listener;
            }
            Err(error) => {
                tracing::info!(?error, port, "Could not bind to port, trying next...");
            }
        }
    };

    axum::serve(listener, app).await.unwrap();
}
