use nata_api::{
    auth::{otp, JwtConfig},
    config::{Config, DEV_JWT_SECRET},
    router, AppState,
};
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    // .env is optional and never overrides variables that are already set.
    let _ = dotenvy::dotenv();

    let config = Config::from_env();

    if config.jwt_secret == DEV_JWT_SECRET {
        tracing::warn!("JWT_SECRET is not set, using the development secret");
    }

    if config.hash_pepper == config.jwt_secret {
        tracing::warn!(
            "HASH_PEPPER is not set: OTP and invitation hashes share JWT_SECRET, \
             so rotating the signing secret invalidates pending ones"
        );
    }

    let mut jwt = JwtConfig::new(config.jwt_secret.clone(), config.jwt_ttl_seconds);

    match config.jwt_secret_prev.as_deref() {
        Some(previous) if previous == config.jwt_secret => {
            tracing::warn!("JWT_SECRET_PREV equals JWT_SECRET; the rotation window is a no-op");
        }
        Some(previous) => {
            tracing::info!("accepting the previous JWT secret until its tokens expire");
            jwt = jwt.with_previous(previous.to_string());
        }
        None => {}
    }

    if config.dev_otp_code.is_some() {
        if cfg!(debug_assertions) {
            tracing::warn!("DEV_OTP_CODE is set: any contact can sign in with the fixed code");
        } else {
            tracing::warn!("DEV_OTP_CODE is set but ignored: release builds always randomize");
        }
    }

    let db = match config.database_url.as_deref() {
        Some(url) => match nata_db::connect(url).await {
            Ok(pool) => {
                tracing::info!("database connection established");
                Some(pool)
            }
            Err(error) => {
                tracing::warn!(%error, "database unavailable, starting without it");
                None
            }
        },
        None => {
            tracing::info!("DATABASE_URL is not set, starting without a database");
            None
        }
    };

    let otp = otp::build(&config);

    let state = AppState {
        db,
        jwt,
        hash_pepper: config.hash_pepper.clone(),
        otp,
        otp_ttl_seconds: config.otp_ttl_seconds,
        jwt_ttl_seconds: config.jwt_ttl_seconds,
        dev_otp_code: config.dev_otp_code,
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!(%addr, "nata-api listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to listen for shutdown signal");
    }
}
