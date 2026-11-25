//! Production suggestions.
//!
//! A product is made to cover the next `days_ahead` of sales:
//!
//! ```text
//! target      = average_daily_sales × days_ahead
//! recommended = max(0, target - sellable_stock)
//! ```
//!
//! Finished goods expire, so the run is capped at what can be sold before it
//! goes past its date. Producing for tomorrow a product that expires in three
//! days is safe; producing a week of it is not:
//!
//! ```text
//! cap = max(0, average_daily_sales × shelf_life_days - sellable_stock)
//! ```
//!
//! The result rounds **up** to three decimals, the batch ledger's scale: a run
//! slightly larger is cheaper than a second run mid-service.
//!
//! Sales over a window are the same shape of division as material usage over a
//! window, but the two answer different questions and carry different errors,
//! so each module states its own.

use std::fmt;

use rust_decimal::{Decimal, RoundingStrategy};

/// Quantities are stored as `numeric(18,3)` in the batch ledger.
const SCALE: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionPlanError {
    /// Stock on hand and the daily rate cannot be negative.
    NegativeQuantity,
    /// The plan looks ahead a number of days, never backwards.
    NegativeDaysAhead,
    /// Shelf life is a number of days, never negative.
    InvalidShelfLife,
    /// A rate needs a window of at least one day.
    InvalidWindow,
}

impl fmt::Display for ProductionPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProductionPlanError::NegativeQuantity => {
                formatter.write_str("quantity cannot be negative")
            }
            ProductionPlanError::NegativeDaysAhead => {
                formatter.write_str("days ahead cannot be negative")
            }
            ProductionPlanError::InvalidShelfLife => {
                formatter.write_str("shelf life cannot be negative")
            }
            ProductionPlanError::InvalidWindow => {
                formatter.write_str("the sales window cannot be zero days")
            }
        }
    }
}

impl std::error::Error for ProductionPlanError {}

/// Everything the plan needs about one product.
#[derive(Debug, Clone, Copy)]
pub struct ProductionInput {
    /// Stock that can still be sold. Expired batches are already excluded, so
    /// they do not reduce the suggestion.
    pub sellable_stock: Decimal,
    pub average_daily_sales: Decimal,
    /// How many days of sales the run is meant to cover.
    pub days_ahead: i32,
    pub shelf_life_days: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionPlan {
    /// Sales the run is meant to cover, before the shelf-life cap.
    pub target_qty: Decimal,
    /// How much to make now.
    pub recommended_qty: Decimal,
    /// True when shelf life, not the target, decided the quantity.
    pub capped_by_shelf_life: bool,
}

/// Units sold per day over `window_days`, which is the tenant's setting. The
/// rate is exact: rounding belongs to the quantity, not to the average.
pub fn average_daily_sales(
    total_sold: Decimal,
    window_days: u32,
) -> Result<Decimal, ProductionPlanError> {
    if window_days == 0 {
        return Err(ProductionPlanError::InvalidWindow);
    }

    if total_sold.is_sign_negative() {
        return Err(ProductionPlanError::NegativeQuantity);
    }

    Ok(total_sold / Decimal::from(window_days))
}

pub fn production_plan(input: ProductionInput) -> Result<ProductionPlan, ProductionPlanError> {
    if input.sellable_stock.is_sign_negative() || input.average_daily_sales.is_sign_negative() {
        return Err(ProductionPlanError::NegativeQuantity);
    }

    if input.days_ahead < 0 {
        return Err(ProductionPlanError::NegativeDaysAhead);
    }

    if let Some(days) = input.shelf_life_days {
        if days <= 0 {
            return Err(ProductionPlanError::InvalidShelfLife);
        }
    }

    let target = input.average_daily_sales * Decimal::from(input.days_ahead);
    let mut recommended = (target - input.sellable_stock).max(Decimal::ZERO);
    let mut capped_by_shelf_life = false;

    // Without sales there is no rate to extrapolate, so the cap would decide
    // everything and a quiet product would never be made again.
    if input.average_daily_sales > Decimal::ZERO {
        if let Some(days) = input.shelf_life_days {
            let cover = input.average_daily_sales * Decimal::from(days);
            let limit = (cover - input.sellable_stock).max(Decimal::ZERO);

            if recommended > limit {
                recommended = limit;
                capped_by_shelf_life = true;
            }
        }
    }

    Ok(ProductionPlan {
        target_qty: round_up(target),
        recommended_qty: round_up(recommended),
        capped_by_shelf_life,
    })
}

fn round_up(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(SCALE, RoundingStrategy::ToPositiveInfinity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(value: &str) -> Decimal {
        value.parse().expect("decimal")
    }

    fn input(stock: &str, sales: &str, days: i32, shelf: Option<i32>) -> ProductionInput {
        ProductionInput {
            sellable_stock: dec(stock),
            average_daily_sales: dec(sales),
            days_ahead: days,
            shelf_life_days: shelf,
        }
    }

    #[test]
    fn the_run_covers_the_days_ahead() {
        // 20 sold a day, one day ahead, 5 already on the shelf.
        let plan = production_plan(input("5", "20", 1, None)).expect("plan");

        assert_eq!(plan.target_qty, dec("20"));
        assert_eq!(plan.recommended_qty, dec("15"));
        assert!(!plan.capped_by_shelf_life);
    }

    #[test]
    fn stock_that_already_covers_the_window_needs_no_run() {
        let plan = production_plan(input("40", "20", 1, None)).expect("plan");

        assert_eq!(plan.target_qty, dec("20"));
        assert_eq!(plan.recommended_qty, Decimal::ZERO);
    }

    #[test]
    fn expired_stock_does_not_reduce_the_run() {
        // The caller passes sellable stock only, so a product whose whole stock
        // is past its date plans as if the shelf were empty.
        let plan = production_plan(input("0", "9", 1, None)).expect("plan");

        assert_eq!(plan.recommended_qty, dec("9"));
    }

    #[test]
    fn shelf_life_caps_a_run_that_would_outlast_the_product() {
        // 10 a day, a 2-day shelf life, nothing on the shelf. A 5-day run would
        // make 50, of which only the first 20 can be sold in time.
        let plan = production_plan(input("0", "10", 5, Some(2))).expect("plan");

        assert_eq!(plan.target_qty, dec("50"));
        assert_eq!(plan.recommended_qty, dec("20"));
        assert!(plan.capped_by_shelf_life);
    }

    #[test]
    fn a_short_run_is_not_capped() {
        // Tomorrow's run expires long before it would, so the target decides.
        let plan = production_plan(input("0", "10", 1, Some(3))).expect("plan");

        assert_eq!(plan.recommended_qty, dec("10"));
        assert!(!plan.capped_by_shelf_life);
    }

    #[test]
    fn a_cap_that_does_not_bite_leaves_the_flag_clear() {
        let plan = production_plan(input("0", "2", 1, Some(30))).expect("plan");

        assert_eq!(plan.recommended_qty, dec("2"));
        assert!(!plan.capped_by_shelf_life);
    }

    #[test]
    fn no_sales_history_skips_the_cap() {
        // A product that has not sold has no rate, so the shelf-life cap would
        // decide everything and nothing would ever be made again.
        let plan = production_plan(input("0", "0", 3, Some(2))).expect("plan");

        assert_eq!(plan.target_qty, Decimal::ZERO);
        assert_eq!(plan.recommended_qty, Decimal::ZERO);
        assert!(!plan.capped_by_shelf_life);
    }

    #[test]
    fn a_stock_lower_than_the_cap_is_subtracted_from_it() {
        // Shelf life covers 30, of which 8 are already sellable.
        let plan = production_plan(input("8", "10", 5, Some(3))).expect("plan");

        assert_eq!(plan.recommended_qty, dec("22"));
        assert!(plan.capped_by_shelf_life);
    }

    #[test]
    fn quantities_round_up_to_the_ledger_scale() {
        let plan = production_plan(input("0", "0.1234", 1, None)).expect("plan");

        assert_eq!(plan.recommended_qty, dec("0.124"));
    }

    #[test]
    fn a_zero_day_plan_makes_nothing() {
        let plan = production_plan(input("0", "10", 0, None)).expect("plan");

        assert_eq!(plan.target_qty, Decimal::ZERO);
        assert_eq!(plan.recommended_qty, Decimal::ZERO);
    }

    #[test]
    fn the_daily_rate_is_the_window_average() {
        let rate = average_daily_sales(dec("90"), 30).expect("rate");

        assert_eq!(rate, dec("3"));
    }

    #[test]
    fn the_daily_rate_keeps_its_precision() {
        // Rounding the rate would drift the suggestion every day.
        let rate = average_daily_sales(dec("100"), 30).expect("rate");

        assert_eq!(rate, dec("3.3333333333333333333333333333"));
    }

    #[test]
    fn a_zero_day_window_is_rejected() {
        assert_eq!(
            average_daily_sales(dec("10"), 0),
            Err(ProductionPlanError::InvalidWindow)
        );
    }

    #[test]
    fn negative_inputs_are_rejected() {
        assert_eq!(
            production_plan(input("-1", "10", 1, None)),
            Err(ProductionPlanError::NegativeQuantity)
        );
        assert_eq!(
            production_plan(input("0", "-1", 1, None)),
            Err(ProductionPlanError::NegativeQuantity)
        );
        assert_eq!(
            production_plan(input("0", "10", -1, None)),
            Err(ProductionPlanError::NegativeDaysAhead)
        );
        assert_eq!(
            production_plan(input("0", "10", 1, Some(0))),
            Err(ProductionPlanError::InvalidShelfLife)
        );
    }
}
