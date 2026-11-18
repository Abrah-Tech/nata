//! Operator tool for manual payments.
//!
//! D4 sells on the web with manual verification: the tenant records a transfer,
//! the operator checks it against the bank, then runs:
//!
//!     cargo run -p nata-api --bin billing -- list
//!     cargo run -p nata-api --bin billing -- verify <payment-id> --by <label>
//!
//! It needs the same role as alert_job: the pending list spans every tenant,
//! and row level security hides them from a tenant-scoped role. The write itself
//! still runs inside a tenant-scoped transaction.

use anyhow::Context;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let pool = nata_db::connect(&database_url)
        .await
        .context("connect to Postgres")?;

    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("list") => list(&pool).await,
        Some("verify") => {
            let id = args
                .get(1)
                .context("usage: billing verify <payment-id> [--by <label>]")?;
            let id: Uuid = id.parse().context("payment id must be a uuid")?;
            let by = flag(&args, "--by").unwrap_or_else(|| "operator".to_string());

            verify(&pool, id, &by).await
        }
        _ => {
            println!("usage:\n  billing list\n  billing verify <payment-id> [--by <label>]");
            std::process::exit(2);
        }
    }
}

#[derive(sqlx::FromRow)]
struct PendingPayment {
    id: Uuid,
    tenant: String,
    amount: Decimal,
    plan: String,
    method: String,
    reference: Option<String>,
    period_start: NaiveDate,
    period_end: NaiveDate,
}

/// Pending transfers, oldest first, so the queue is worked in order.
async fn list(pool: &PgPool) -> anyhow::Result<()> {
    let rows: Vec<PendingPayment> = sqlx::query_as(
        "select p.id, t.name as tenant, p.amount, p.plan, p.method, p.reference,
                p.period_start, p.period_end
         from payments p
         join tenants t on t.id = p.tenant_id
         where p.status = 'pending'
         order by p.created_at",
    )
    .fetch_all(pool)
    .await
    .context("list pending payments")?;

    if rows.is_empty() {
        println!("no pending payments");
        return Ok(());
    }

    for payment in rows {
        println!(
            "{}  {}  {}  {}  {}..{}  {}  {}",
            payment.id,
            payment.tenant,
            payment.amount,
            payment.plan,
            payment.period_start,
            payment.period_end,
            payment.method,
            payment.reference.unwrap_or_default()
        );
    }

    Ok(())
}

/// Marks a pending payment verified and extends the subscription.
async fn verify(pool: &PgPool, id: Uuid, by: &str) -> anyhow::Result<()> {
    let verified = nata_api::billing::verify_payment(pool, id, by)
        .await
        .map_err(|error| anyhow::anyhow!("{}", error.code()))?;

    println!(
        "verified {id} for {}: plan={} paid_until={}",
        verified.tenant,
        verified.plan,
        verified
            .paid_until
            .map(|date| date.to_rfc3339())
            .unwrap_or_else(|| "-".to_string())
    );

    Ok(())
}

/// Reads --flag value from the arguments.
fn flag(args: &[String], name: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == name)?;

    args.get(index + 1).cloned()
}
