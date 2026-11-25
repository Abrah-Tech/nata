//! Pure calculation core for Nata.
//!
//! This crate owns every number that must be identical on the client and the
//! server: FEFO allocation, material requirements, HPP, unit conversion, and
//! reorder planning.
//!
//! Rules for anything added here:
//! - no IO, no async, no database access;
//! - deterministic input to output;
//! - `rust_decimal` for money and quantities, never `f64`.

pub mod fefo;
pub mod hpp;
pub mod production_plan;
pub mod reorder;
pub mod requirement;

pub use fefo::{allocate, Allocation, AllocationLine, FefoError, StockBatch};
pub use hpp::{production_cost, sold_unit_cost, Consumption, CostError, ProductionCost, SoldBatch};
pub use production_plan::{
    average_daily_sales, production_plan, ProductionInput, ProductionPlan, ProductionPlanError,
};
pub use reorder::{average_daily_usage, reorder_plan, ReorderError, ReorderInput, ReorderPlan};
pub use requirement::{material_requirement, RequirementError};
