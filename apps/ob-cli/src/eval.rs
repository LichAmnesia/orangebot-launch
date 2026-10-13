//! `ob eval` — fixed evaluation suites against a text model.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, bail};
use ob_providers::{ClientConfig, OpenAiClient};

struct Case {
    id: String,
    mode: String,
    target: Option<String>,
    input: String,
    must: Vec<String>,
    must_any: Vec<String>,
    must_not: Vec<String>,
}

fn str_field(v: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    v.get(key)
        .and_then(|f| f.as_str())
        .map(str::to_string)
        .with_context(|| format!("missing field {key}"))
}

fn str_vec(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|f| f.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_case(line: &str, n: usize) -> anyhow::Result<Case> {
    let v: serde_json::Value = serde_json::from_str(line).with_context(|| format!("line {n}"))?;
    Ok(Case {
        id: str_field(&v, "id").with_context(|| format!("line {n}"))?,
        mode: str_field(&v, "mode").with_context(|| format!("line {n}"))?,
        target: v.get("target").and_then(|f| f.as_str()).map(str::to_string),
        input: str_field(&v, "input").with_context(|| format!("line {n}"))?,
        must: str_vec(&v, "must"),
        must_any: str_vec(&v, "must_any"),
        must_not: str_vec(&v, "must_not"),
    })
}

fn median_ms(times: &[u128]) -> u128 {
    if times.is_empty() {
        return 0;
    }
    let mut v = times.to_vec();
    v.sort_unstable();
    let mid = v.len() / 2;
    if v.len() % 2 == 1 {
        v[mid]
    } else {
        (v[mid - 1] + v[mid]) / 2
    }
}

pub async fn run(
    suite: String,
    llm_url: String,
    llm_model: String,
    file: PathBuf,
    min_pass: Option<usize>,
) -> anyhow::Result<()> {
    if suite != "cleanup" {
        bail!("unknown suite {suite}");
    }
    let text =
        std::fs::read_to_string(&file).with_context(|| format!("read {}", file.display()))?;
    let mut cases = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        cases.push(parse_case(line, i + 1)?);
    }

    let client = OpenAiClient::new(ClientConfig {
        base_url: llm_url,
        model: llm_model,
        api_key: std::env::var("OB_LLM_API_KEY").ok(),
        connect_timeout_ms: 5_000,
        total_timeout_ms: 60_000,
    })?;

    let mut passed = 0_usize;
    let mut times: Vec<u128> = Vec::new();
    for case in &cases {
        let Ok(mode) = (|| -> anyhow::Result<ob_type::Mode> {
            Ok(match case.mode.as_str() {
                "dictate" => ob_type::Mode::Dictate,
                "translate" => ob_type::Mode::Translate {
                    target: case.target.clone().unwrap_or_else(|| "English".to_string()),
                },
                "ask" => ob_type::Mode::Ask,
                "rewrite" => ob_type::Mode::Rewrite,
                other => bail!("bad mode {other}"),
            })
        })() else {
            println!("FAIL {:<22} {:>5} ms  bad mode {}", case.id, 0, case.mode);
            continue;
        };

        let t0 = Instant::now();
        let messages = ob_type::build_messages(&mode, &case.input, &[]);
        let out = match client.chat_with(&messages, 512, 0.0, true).await {
            Ok(raw) => ob_type::clean_model_output(&raw),
            Err(e) => {
                let t = t0.elapsed().as_millis();
                times.push(t);
                println!("FAIL {:<22} {:>5} ms  request error: {e}", case.id, t);
                continue;
            }
        };
        let t = t0.elapsed().as_millis();
        times.push(t);

        let hay = out.to_lowercase();
        let mut reasons: Vec<String> = Vec::new();
        for m in &case.must {
            if !hay.contains(&m.to_lowercase()) {
                reasons.push(format!("missing {m}"));
            }
        }
        if !case.must_any.is_empty()
            && !case
                .must_any
                .iter()
                .any(|m| hay.contains(&m.to_lowercase()))
        {
            reasons.push(format!("none of [{}]", case.must_any.join(", ")));
        }
        for m in &case.must_not {
            if hay.contains(&m.to_lowercase()) {
                reasons.push(format!("contains {m}"));
            }
        }

        if reasons.is_empty() {
            passed += 1;
            println!("PASS {:<22} {:>5} ms", case.id, t);
        } else {
            println!(
                "FAIL {:<22} {:>5} ms  {}\n       → {out}",
                case.id,
                t,
                reasons.join("; ")
            );
        }
    }

    let n = cases.len();
    println!("passed {passed}/{n} · median {} ms", median_ms(&times));
    let threshold = min_pass.unwrap_or(n);
    if passed < threshold {
        bail!("{passed}/{n} passed, need {threshold}");
    }
    Ok(())
}
