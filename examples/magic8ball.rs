//! Magic 8-Ball JSON-RPC server.
//!
//! A mystical fortune-telling service with structured error codes for
//! different cosmic failure modes.
//!
//! ## Methods
//!
//! - `shakeBall` — Ask a yes/no question, receive a fortune.
//! - `readAura` — Read the cosmic aura for a named querent.
//! - `consultStars` — Check planetary alignment for a zodiac sign.

use ajj::{ErrorPayload, IntoErrorPayload, Router};
use serde::Serialize;
use std::borrow::Cow;

// ── Fortunes ────────────────────────────────────────────────────────

const POSITIVE: &[&str] = &[
    "It is certain.",
    "Without a doubt.",
    "You may rely on it.",
    "Yes, definitely.",
    "As I see it, yes.",
];

const NEGATIVE: &[&str] = &[
    "Don't count on it.",
    "My reply is no.",
    "My sources say no.",
    "Very doubtful.",
];

const NEUTRAL: &[&str] = &[
    "Reply hazy, try again.",
    "Ask again later.",
    "Better not tell you now.",
    "Cannot predict now.",
    "Concentrate and ask again.",
];

fn pick<'a>(choices: &'a [&'a str], seed: u64) -> &'a str {
    choices[(seed as usize) % choices.len()]
}

fn cheap_hash(s: &str) -> u64 {
    s.bytes()
        .fold(5381u64, |h, b| h.wrapping_mul(33).wrapping_add(b as u64))
}

// ── Error types ─────────────────────────────────────────────────────

/// Structured data returned when the crystal ball is cloudy.
#[derive(Debug, Serialize)]
struct CloudyDetail {
    visibility: &'static str,
    suggestion: &'static str,
}

/// Structured data for planetary misalignment.
#[derive(Debug, Serialize)]
struct MisalignmentDetail {
    sign: String,
    interfering_planet: &'static str,
}

/// All the ways a mystical consultation can fail.
#[derive(Debug)]
enum MysticError {
    /// The crystal ball is too cloudy to read.
    CrystalBallCloudy,
    /// The stars are not aligned for this query.
    StarsMisaligned { sign: String },
    /// Mercury is in retrograde — all bets are off.
    MercuryRetrograde,
    /// The querent's aura is unreadable.
    UnreadableAura(String),
}

impl IntoErrorPayload for MysticError {
    type ErrData = Box<serde_json::value::RawValue>;

    fn error_code(&self) -> i64 {
        match self {
            Self::CrystalBallCloudy => -32001,
            Self::StarsMisaligned { .. } => -32002,
            Self::MercuryRetrograde => -32003,
            Self::UnreadableAura(_) => -32004,
        }
    }

    fn error_message(&self) -> Cow<'static, str> {
        match self {
            Self::CrystalBallCloudy => "Crystal ball is cloudy".into(),
            Self::StarsMisaligned { sign } => format!("Stars misaligned for {sign}").into(),
            Self::MercuryRetrograde => "Mercury is in retrograde".into(),
            Self::UnreadableAura(name) => format!("Cannot read aura of {name}").into(),
        }
    }

    fn into_error_payload(self) -> ErrorPayload<Box<serde_json::value::RawValue>> {
        let code = self.error_code();
        let message = self.error_message();
        let data = match &self {
            Self::CrystalBallCloudy => serde_json::value::to_raw_value(&CloudyDetail {
                visibility: "opaque",
                suggestion: "try polishing the ball",
            })
            .ok(),
            Self::StarsMisaligned { sign } => {
                serde_json::value::to_raw_value(&MisalignmentDetail {
                    sign: sign.clone(),
                    interfering_planet: "Saturn",
                })
                .ok()
            }
            Self::MercuryRetrograde | Self::UnreadableAura(_) => None,
        };
        ErrorPayload {
            code,
            message,
            data,
        }
    }
}

// ── Handlers ────────────────────────────────────────────────────────

/// Shake the Magic 8-Ball with a yes/no question.
async fn shake_ball(question: String) -> Result<String, MysticError> {
    if question.is_empty() {
        return Err(MysticError::CrystalBallCloudy);
    }

    let h = cheap_hash(&question);
    let fortune = match h % 3 {
        0 => pick(POSITIVE, h),
        1 => pick(NEGATIVE, h),
        _ => pick(NEUTRAL, h),
    };
    Ok(fortune.to_string())
}

/// Read the cosmic aura for a named querent.
async fn read_aura(name: String) -> Result<String, MysticError> {
    if name.len() < 2 {
        return Err(MysticError::UnreadableAura(name));
    }

    let h = cheap_hash(&name);
    let colors = ["violet", "indigo", "gold", "emerald", "crimson", "silver"];
    let intensity = ["faint", "steady", "brilliant", "pulsing", "radiant"];

    Ok(format!(
        "{}'s aura is {} {}",
        name,
        intensity[(h as usize) % intensity.len()],
        colors[(h as usize / 7) % colors.len()],
    ))
}

/// Check planetary alignment for a zodiac sign.
async fn consult_stars(sign: String) -> Result<String, MysticError> {
    let valid = [
        "aries",
        "taurus",
        "gemini",
        "cancer",
        "leo",
        "virgo",
        "libra",
        "scorpio",
        "sagittarius",
        "capricorn",
        "aquarius",
        "pisces",
    ];

    let lower = sign.to_lowercase();
    if !valid.contains(&lower.as_str()) {
        return Err(MysticError::StarsMisaligned { sign });
    }

    // Mercury retrograde hits Gemini and Virgo (Mercury-ruled signs)
    if lower == "gemini" || lower == "virgo" {
        return Err(MysticError::MercuryRetrograde);
    }

    let h = cheap_hash(&lower);
    let forecasts = [
        "The cosmos smile upon you today.",
        "A journey of great importance awaits.",
        "Trust the process — transformation is near.",
        "An unexpected ally will appear.",
        "Financial winds blow in your favor.",
    ];

    Ok(format!(
        "{}: {}",
        sign,
        forecasts[(h as usize) % forecasts.len()]
    ))
}

// ── Server ──────────────────────────────────────────────────────────

fn make_router() -> Router<()> {
    Router::new()
        .route("shakeBall", shake_ball)
        .route("readAura", read_aura)
        .route("consultStars", consult_stars)
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let router = make_router();
    let axum = router.into_axum("/");

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8545));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    println!("Magic 8-Ball RPC listening on http://{}", addr);
    println!();
    println!("Try:");
    println!(r#"  curl -X POST http://localhost:8545 -H 'Content-Type: application/json' \"#);
    println!(
        r#"    -d '{{"jsonrpc":"2.0","id":1,"method":"shakeBall","params":"Will I be lucky today?"}}'"#
    );
    println!();
    println!(r#"  curl -X POST http://localhost:8545 -H 'Content-Type: application/json' \"#);
    println!(r#"    -d '{{"jsonrpc":"2.0","id":2,"method":"readAura","params":"Merlin"}}'"#);
    println!();
    println!(r#"  curl -X POST http://localhost:8545 -H 'Content-Type: application/json' \"#);
    println!(r#"    -d '{{"jsonrpc":"2.0","id":3,"method":"consultStars","params":"gemini"}}'"#);

    axum::serve(listener, axum).await.map_err(Into::into)
}
