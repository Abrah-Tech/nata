//! Cost of a production run (HPP).
//!
//! HPP per unit is what the run cost divided by what it actually produced:
//!
//! ```text
//! unit_cost = (material + labor + overhead) / actual_qty
//! ```
//!
//! Material cost is the sum of the consumption snapshots, so a purchase at a
//! different price later cannot change a past run. Components round to two
//! decimals (the money columns) before the division, so the stored components
//! always add up to the stored unit cost. The unit cost keeps four decimals
//! (D7).

use std::fmt;

use rust_decimal::Decimal;

/// Money is stored as `numeric(18,2)`.
const MONEY_SCALE: u32 = 2;
/// HPP per unit is stored as `numeric(18,4)`.
const UNIT_COST_SCALE: u32 = 4;

/// One batch consumed by the run, with the cost it had at that moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Consumption {
    pub qty: Decimal,
    pub unit_cost: Decimal,
}

/// The cost snapshot of one production run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionCost {
    pub material_cost: Decimal,
    pub labor_cost: Decimal,
    pub overhead_cost: Decimal,
    /// Cost per finished unit.
    pub unit_cost: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostError {
    /// A run must produce something; dividing by zero has no meaning.
    InvalidQuantity,
    /// Costs and consumed quantities cannot be negative.
    NegativeCost,
    /// A recipe's labor minutes are a duration, so they cannot be negative.
    InvalidMinutes,
}

impl fmt::Display for CostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CostError::InvalidQuantity => {
                formatter.write_str("production quantity must be positive")
            }
            CostError::NegativeCost => formatter.write_str("cost cannot be negative"),
            CostError::InvalidMinutes => {
                formatter.write_str("production minutes cannot be negative")
            }
        }
    }
}

impl std::error::Error for CostError {}

/// Cost of one hourly component, labor or overhead, over a run.
///
/// A recipe's `minutes` covers one batch, so a run of `batches` batches takes
/// `minutes * batches / 60` hours. Both components use this function, which is
/// what keeps them on the same production hours: the recipe is read once and
/// the two rates only scale the result. The amount is money, so it is rounded
/// to two decimals here; [`production_cost`] rounds again, which changes
/// nothing.
pub fn hourly_cost(
    rate_per_hour: Decimal,
    minutes: i32,
    batches: Decimal,
) -> Result<Decimal, CostError> {
    if rate_per_hour.is_sign_negative() || batches.is_sign_negative() {
        return Err(CostError::NegativeCost);
    }
    if minutes < 0 {
        return Err(CostError::InvalidMinutes);
    }

    let hours = Decimal::from(minutes) / Decimal::from(60);
    Ok((rate_per_hour * hours * batches).round_dp(MONEY_SCALE))
}

/// Snapshots the cost of a run from its consumptions, labor, and overhead.
///
/// `actual_qty` is what the run really produced, not what was planned.
pub fn production_cost(
    consumptions: &[Consumption],
    labor_cost: Decimal,
    overhead_cost: Decimal,
    actual_qty: Decimal,
) -> Result<ProductionCost, CostError> {
    if actual_qty <= Decimal::ZERO {
        return Err(CostError::InvalidQuantity);
    }

    if labor_cost.is_sign_negative() || overhead_cost.is_sign_negative() {
        return Err(CostError::NegativeCost);
    }

    let mut material = Decimal::ZERO;

    for consumption in consumptions {
        if consumption.qty.is_sign_negative() || consumption.unit_cost.is_sign_negative() {
            return Err(CostError::NegativeCost);
        }

        material += consumption.qty * consumption.unit_cost;
    }

    let material_cost = material.round_dp(MONEY_SCALE);
    let labor_cost = labor_cost.round_dp(MONEY_SCALE);
    let overhead_cost = overhead_cost.round_dp(MONEY_SCALE);
    let total = material_cost + labor_cost + overhead_cost;
    let unit_cost = (total / actual_qty).round_dp(UNIT_COST_SCALE);

    Ok(ProductionCost {
        material_cost,
        labor_cost,
        overhead_cost,
        unit_cost,
    })
}

/// One batch a sale took, with the cost it had at that moment. A `None` cost
/// means the batch's cost is unknown, so the sale cannot be valued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoldBatch {
    pub qty: Decimal,
    pub unit_cost: Option<Decimal>,
}

/// Weighted cost per sold unit across the batches a sale took.
///
/// Returns `None` when any batch has no known cost, so the caller marks the
/// sale as cost incomplete instead of inventing a number. Four decimals, the
/// same scale as every other unit cost (D7).
pub fn sold_unit_cost(
    lines: &[SoldBatch],
    sold_qty: Decimal,
) -> Result<Option<Decimal>, CostError> {
    if sold_qty <= Decimal::ZERO {
        return Err(CostError::InvalidQuantity);
    }

    let mut total = Decimal::ZERO;

    for line in lines {
        if line.qty.is_sign_negative() {
            return Err(CostError::NegativeCost);
        }

        match line.unit_cost {
            Some(cost) if cost.is_sign_negative() => return Err(CostError::NegativeCost),
            Some(cost) => total += line.qty * cost,
            None => return Ok(None),
        }
    }

    Ok(Some((total / sold_qty).round_dp(UNIT_COST_SCALE)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(value: &str) -> Decimal {
        value.parse().expect("decimal")
    }

    fn consumption(qty: &str, unit_cost: &str) -> Consumption {
        Consumption {
            qty: dec(qty),
            unit_cost: dec(unit_cost),
        }
    }

    #[test]
    fn material_cost_sums_the_consumption_snapshots() {
        let cost = production_cost(
            &[consumption("1", "12000"), consumption("1.625", "9000")],
            Decimal::ZERO,
            Decimal::ZERO,
            dec("10"),
        )
        .expect("cost");

        assert_eq!(cost.material_cost, dec("26625.00"));
        assert_eq!(cost.unit_cost, dec("2662.5000"));
    }

    #[test]
    fn labor_and_overhead_are_added_before_dividing() {
        let cost = production_cost(
            &[consumption("1", "10000")],
            dec("5000"),
            dec("2500"),
            dec("10"),
        )
        .expect("cost");

        assert_eq!(cost.material_cost, dec("10000.00"));
        assert_eq!(cost.labor_cost, dec("5000.00"));
        assert_eq!(cost.overhead_cost, dec("2500.00"));
        assert_eq!(cost.unit_cost, dec("1750.0000"));
    }

    #[test]
    fn unit_cost_keeps_four_decimals() {
        let cost = production_cost(
            &[consumption("1", "10000")],
            Decimal::ZERO,
            Decimal::ZERO,
            dec("3"),
        )
        .expect("cost");

        assert_eq!(cost.unit_cost, dec("3333.3333"));
    }

    #[test]
    fn components_round_to_money_before_dividing() {
        let cost = production_cost(
            &[consumption("0.333", "0.333")],
            Decimal::ZERO,
            Decimal::ZERO,
            dec("1"),
        )
        .expect("cost");

        assert_eq!(cost.material_cost, dec("0.11"));
        assert_eq!(cost.unit_cost, dec("0.1100"));
    }

    #[test]
    fn an_hourly_rate_is_priced_over_the_run_minutes() {
        // Half an hour at 25 000 is 12 500 for the one batch the run makes.
        assert_eq!(
            hourly_cost(dec("25000"), 30, dec("1")).expect("cost"),
            dec("12500.00")
        );

        // Two batches of a half-hour recipe take three quarters of an hour.
        assert_eq!(
            hourly_cost(dec("25000"), 30, dec("1.5")).expect("cost"),
            dec("18750.00")
        );
    }

    #[test]
    fn an_hourly_cost_rounds_to_money() {
        // 1000 an hour for 10 minutes is 166.666…, which stores as 166.67.
        assert_eq!(
            hourly_cost(dec("1000"), 10, dec("1")).expect("cost"),
            dec("166.67")
        );
    }

    #[test]
    fn a_zero_rate_costs_nothing() {
        assert_eq!(
            hourly_cost(Decimal::ZERO, 30, dec("2")).expect("cost"),
            dec("0")
        );
    }

    #[test]
    fn an_hourly_cost_rejects_negative_inputs() {
        assert_eq!(
            hourly_cost(dec("-1"), 30, dec("1")),
            Err(CostError::NegativeCost)
        );

        assert_eq!(
            hourly_cost(dec("1000"), 30, dec("-1")),
            Err(CostError::NegativeCost)
        );

        assert_eq!(
            hourly_cost(dec("1000"), -1, dec("1")),
            Err(CostError::InvalidMinutes)
        );
    }

    #[test]
    fn zero_labor_and_overhead_leave_the_material_cost_alone() {
        let cost = production_cost(
            &[consumption("2.5", "8000")],
            Decimal::ZERO,
            Decimal::ZERO,
            dec("5"),
        )
        .expect("cost");

        assert_eq!(cost.unit_cost, dec("4000.0000"));
    }

    #[test]
    fn a_zero_or_negative_quantity_is_rejected() {
        for qty in ["0", "-1"] {
            assert_eq!(
                production_cost(&[], Decimal::ZERO, Decimal::ZERO, dec(qty)),
                Err(CostError::InvalidQuantity)
            );
        }
    }

    #[test]
    fn negative_costs_are_rejected() {
        assert_eq!(
            production_cost(&[], dec("-1"), Decimal::ZERO, dec("1")),
            Err(CostError::NegativeCost)
        );

        assert_eq!(
            production_cost(&[], Decimal::ZERO, dec("-1"), dec("1")),
            Err(CostError::NegativeCost)
        );

        assert_eq!(
            production_cost(
                &[consumption("-1", "1000")],
                Decimal::ZERO,
                Decimal::ZERO,
                dec("1")
            ),
            Err(CostError::NegativeCost)
        );

        assert_eq!(
            production_cost(
                &[consumption("1", "-1000")],
                Decimal::ZERO,
                Decimal::ZERO,
                dec("1")
            ),
            Err(CostError::NegativeCost)
        );
    }

    fn sold(qty: &str, unit_cost: Option<&str>) -> SoldBatch {
        SoldBatch {
            qty: dec(qty),
            unit_cost: unit_cost.map(dec),
        }
    }

    #[test]
    fn sold_cost_is_the_quantity_weighted_average() {
        let cost = sold_unit_cost(
            &[sold("1", Some("10000")), sold("1", Some("20000"))],
            dec("2"),
        )
        .expect("cost");

        assert_eq!(cost, Some(dec("15000.0000")));
    }

    #[test]
    fn a_single_batch_keeps_its_own_cost() {
        let cost = sold_unit_cost(&[sold("3", Some("12000"))], dec("3")).expect("cost");

        assert_eq!(cost, Some(dec("12000.0000")));
    }

    #[test]
    fn an_unknown_batch_cost_makes_the_sale_incomplete() {
        let cost =
            sold_unit_cost(&[sold("1", Some("10000")), sold("1", None)], dec("2")).expect("cost");

        assert_eq!(cost, None);
    }

    #[test]
    fn sold_cost_rejects_bad_quantities_and_costs() {
        assert_eq!(
            sold_unit_cost(&[sold("1", Some("10000"))], dec("0")),
            Err(CostError::InvalidQuantity)
        );

        assert_eq!(
            sold_unit_cost(&[sold("-1", Some("10000"))], dec("1")),
            Err(CostError::NegativeCost)
        );

        assert_eq!(
            sold_unit_cost(&[sold("1", Some("-10000"))], dec("1")),
            Err(CostError::NegativeCost)
        );
    }
}
