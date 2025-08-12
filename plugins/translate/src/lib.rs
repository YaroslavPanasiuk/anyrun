use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;
use reqwest::Client;
use tokio::runtime::Runtime;

#[derive(Default)]
struct Config {
    prefix: String,
}

struct State {
    config: Config,
    client: Client,
    runtime: Runtime,
}

#[init]
fn init(_config_dir: RString) -> State {
    State {
        config: Config {
            prefix: "".to_string(),
        },
        client: Client::new(),
        runtime: Runtime::new().expect("Failed to create tokio runtime"),
    }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "Translate".into(),
        icon: "preferences-desktop-locale".into(),
    }
}

#[get_matches]
fn get_matches(input: RString, state: &State) -> RVec<Match> {
    if !input.starts_with(&state.config.prefix) {
        return RVec::new();
    }

    // Ignore the prefix
    let text = input[state.config.prefix.len()..].trim();
    
    if text.is_empty() {
        return RVec::new();
    }

    // Try to detect if the text is Ukrainian (we'll assume English otherwise)
    let is_ukrainian = text.chars().any(|c| {
        let cp = c as u32;
        // Check for Cyrillic characters used in Ukrainian
        (cp >= 0x0400 && cp <= 0x04FF) ||  // Cyrillic range
        (cp >= 0x0500 && cp <= 0x052F)     // Cyrillic Supplement
    });

    let (src_lang, dest_lang) = if is_ukrainian {
        ("uk", "en")
    } else {
        ("en", "uk")
    };

    state.runtime.block_on(async move {
        let response = state.client
            .get(format!(
                "https://translate.googleapis.com/translate_a/single?client=gtx&sl={}&tl={}&dt=t&q={}",
                src_lang, dest_lang, text
            ))
            .send()
            .await;

        match response {
            Ok(resp) => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    let translation = json[0]
                        .as_array()
                        .expect("Malformed JSON!")
                        .iter()
                        .map(|val| val.as_array().expect("Malformed JSON!")[0].as_str()
                            .expect("Malformed JSON!")
                        )
                        .collect::<Vec<_>>()
                        .join(" ");

                    let detected_lang = json[2].as_str().unwrap_or(src_lang);
                    let lang_name = if detected_lang == "uk" { "Ukrainian" } else { "English" };
                    let dest_name = if dest_lang == "uk" { "Ukrainian" } else { "English" };

                    RVec::from(vec![Match {
                        title: translation.into(),
                        description: ROption::RSome(
                            format!("{} → {}", lang_name, dest_name).into()
                        ),
                        use_pango: false,
                        icon: ROption::RNone,
                        id: ROption::RNone,
                    }])
                } else {
                    RVec::new()
                }
            }
            Err(_) => RVec::new(),
        }
    })
}

#[handler]
fn handler(selection: Match) -> HandleResult {
    HandleResult::Copy(selection.title.into_bytes())
}
