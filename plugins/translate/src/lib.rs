use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;
use reqwest::Client;
use serde::Deserialize;
use tokio::runtime::Runtime;

#[derive(Deserialize)]
struct Config {
    prefix: String,
    max_entries: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefix: ":".to_string(),
            max_entries: 3,
        }
    }
}

struct State {
    config: Config,
    client: Client,
    runtime: Runtime,
}

#[init]
fn init(config_dir: RString) -> State {
    State {
        config: match std::fs::read_to_string(format!("{}/translate.ron", config_dir)) {
            Ok(content) => ron::from_str(&content).unwrap_or_default(),
            Err(_) => Config::default(),
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
    let text = &input[state.config.prefix.len()..];
    
    if text.is_empty() {
        return RVec::new();
    }

    state.runtime.block_on(async move {
        // First detect the language
        let detection = state.client
            .get(format!(
                "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl=en&dt=ld&q={}",
                text
            ))
            .send()
            .await;

        let detected_lang = match detection {
            Ok(response) => {
                let json: serde_json::Value = response.json().await.unwrap_or_default();
                json.get("src").and_then(|v| v.as_str()).unwrap_or("en")
            }
            Err(_) => "en",
        };

        // Create translation futures based on detected language
        let (sl, tl) = if detected_lang == "uk" {
            ("uk", "en")
        } else {
            ("en", "uk")
        };

        async fn get_translation(
            client: &Client,
            name: &'static str,
            sl: &str,
            tl: &str,
            text: &str,
        ) -> (&'static str, reqwest::Result<reqwest::Response>) {
            (
                name,
                client
                    .get(format!(
                        "https://translate.googleapis.com/translate_a/single?client=gtx&sl={}&tl={}&dt=t&q={}",
                        sl, tl, text
                    ))
                    .send()
                    .await,
            )
        }

        // Create the futures for both translation directions and auto-detect
        let futures = [
            get_translation(&state.client, "English → Ukrainian", "en", "uk", text),
            get_translation(&state.client, "Ukrainian → English", "uk", "en", text),
            get_translation(&state.client, "Auto-detect", sl, tl, text),
        ];

        let results = futures::future::join_all(futures).await;

        results
            .into_iter()
            .filter_map(|(name, res)| {
                res.ok()
                    .map(|response| {
                        futures::executor::block_on(response.json())
                            .ok()
                            .map(|json: serde_json::Value| {
                                let translation = json[0]
                                    .as_array()
                                    .expect("Malformed JSON!")
                                    .iter()
                                    .map(|val| {
                                        val.as_array()
                                            .expect("Malformed JSON!")[0]
                                            .as_str()
                                            .expect("Malformed JSON!")
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                
                                let description = if name == "Auto-detect" {
                                    let direction = if detected_lang == "en" {
                                        "English → Ukrainian"
                                    } else {
                                        "Ukrainian → English"
                                    };
                                    format!("{} (detected: {})", direction, detected_lang)
                                } else {
                                    name.to_string()
                                };

                                Match {
                                    title: translation.into(),
                                    description: ROption::RSome(description.into()),
                                    use_pango: false,
                                    icon: ROption::RNone,
                                    id: ROption::RNone,
                                }
                            })
                    })
                    .flatten()
            })
            .take(state.config.max_entries)
            .collect::<RVec<_>>()
    })
}

#[handler]
fn handler(selection: Match) -> HandleResult {
    HandleResult::Copy(selection.title.into_bytes())
}
