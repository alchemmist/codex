use antex_core::Credits;
use antex_core::Quota;
use antex_core::QuotaWindow;
use reqwest::header::HeaderMap;
use serde_json::Value;
use serde_json::json;

pub(crate) fn event(value: &Value) -> Quota {
    Quota {
        primary: window(&value["rate_limits"]["primary"]),
        secondary: window(&value["rate_limits"]["secondary"]),
        credits: value["credits"].as_object().map(|credits| Credits {
            available: credits
                .get("has_credits")
                .and_then(Value::as_bool)
                .unwrap_or_default(),
            unlimited: credits
                .get("unlimited")
                .and_then(Value::as_bool)
                .unwrap_or_default(),
            balance: credits
                .get("balance")
                .and_then(Value::as_str)
                .filter(|balance| balance.len() <= 64)
                .map(str::to_owned),
        }),
    }
}

pub(crate) fn headers(headers: &HeaderMap) -> Option<Quota> {
    let mut value = json!({"rate_limits":{}});
    for name in ["primary", "secondary"] {
        let used = headers
            .get(format!("x-codex-{name}-used-percent"))
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<f64>().ok());
        if let Some(used) = used.filter(|used| used.is_finite()) {
            let minutes = headers
                .get(format!("x-codex-{name}-window-minutes"))
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let reset = headers
                .get(format!("x-codex-{name}-reset-at"))
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            value["rate_limits"][name] =
                json!({"used_percent":used,"window_minutes":minutes,"reset_at":reset});
        }
    }
    let quota = event(&value);
    (quota != Quota::default()).then_some(quota)
}

fn window(value: &Value) -> Option<QuotaWindow> {
    let used = value["used_percent"]
        .as_f64()
        .filter(|used| used.is_finite())?;
    Some(QuotaWindow {
        used_basis_points: (used.clamp(0.0, 100.0) * 100.0).round() as u16,
        window_seconds: value["window_minutes"]
            .as_u64()
            .and_then(|minutes| minutes.checked_mul(60)),
        resets_at: value["reset_at"].as_u64(),
    })
}
