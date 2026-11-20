//! Starter catalogs for a new tenant.
//!
//! Reference data, like the unit list: the product owns it, the tenant applies
//! it. Items are seeded as ordinary materials, so everything downstream treats
//! them the same as hand-typed ones.
//!
//! A template is a bundle, not a material list. Materials alone leave the
//! tenant unable to produce anything, so each template also brings a few
//! finished products with an active recipe whose items point at that
//! template's own materials by name. Applying a template therefore takes a
//! business from an empty tenant to one that can run a production order.

use axum::{
    extract::{Path, State},
    Json,
};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use uuid::Uuid;

use crate::auth::guard::{CanReadMaterials, CanWriteMaterials, Require};
use crate::error::ApiError;
use crate::AppState;

use super::ImportResult;

pub struct TemplateItem {
    pub name: &'static str,
    pub unit: &'static str,
    pub min_stock: &'static str,
    pub shelf_life_days: Option<i32>,
}

/// One finished product a template brings, with the recipe that produces it.
pub struct TemplateProduct {
    pub name: &'static str,
    pub sell_price: &'static str,
    pub shelf_life_days: Option<i32>,
    /// Output of one run of `recipe`.
    pub yield_qty: &'static str,
    pub labor_minutes: i32,
    pub items: &'static [TemplateProductItem],
}

pub struct TemplateProductItem {
    /// Names a row of the same template's material list, so the recipe is
    /// written against the material this template stands up.
    pub material: &'static str,
    pub qty: &'static str,
    /// Kept in the material's base unit, exactly like a batch.
    pub waste_pct: &'static str,
}

pub struct Template {
    pub id: &'static str,
    pub items: &'static [TemplateItem],
    pub products: &'static [TemplateProduct],
}

const fn item(
    name: &'static str,
    unit: &'static str,
    min_stock: &'static str,
    shelf_life_days: Option<i32>,
) -> TemplateItem {
    TemplateItem {
        name,
        unit,
        min_stock,
        shelf_life_days,
    }
}

const fn product_item(
    material: &'static str,
    qty: &'static str,
    waste_pct: &'static str,
) -> TemplateProductItem {
    TemplateProductItem {
        material,
        qty,
        waste_pct,
    }
}

#[allow(clippy::too_many_arguments)]
const fn product(
    name: &'static str,
    sell_price: &'static str,
    shelf_life_days: Option<i32>,
    yield_qty: &'static str,
    labor_minutes: i32,
    items: &'static [TemplateProductItem],
) -> TemplateProduct {
    TemplateProduct {
        name,
        sell_price,
        shelf_life_days,
        yield_qty,
        labor_minutes,
        items,
    }
}

const BAKERY: &[TemplateItem] = &[
    item("Tepung terigu protein tinggi", "kg", "5", Some(180)),
    item("Tepung terigu protein sedang", "kg", "5", Some(180)),
    item("Tepung maizena", "kg", "1", Some(365)),
    item("Gula pasir", "kg", "3", Some(730)),
    item("Gula halus", "kg", "1", Some(365)),
    item("Mentega tawar", "kg", "2", Some(90)),
    item("Margarin", "kg", "1", Some(120)),
    item("Telur ayam", "kg", "5", Some(21)),
    item("Susu UHT", "l", "4", Some(120)),
    item("Krim kental manis", "pcs", "6", Some(365)),
    item("Ragi instan", "g", "500", Some(365)),
    item("Baking powder", "g", "500", Some(365)),
    item("Baking soda", "g", "250", Some(730)),
    item("Garam", "kg", "1", Some(1825)),
    item("Cokelat compound", "kg", "2", Some(365)),
    item("Keju cheddar", "kg", "1", Some(180)),
    item("Vanili", "g", "100", Some(730)),
    item("Pewarna makanan", "ml", "200", Some(730)),
    item("Kertas roti", "pcs", "50", None),
    item("Box kue", "pcs", "100", None),
];

const BAKERY_PRODUCTS: &[TemplateProduct] = &[
    product(
        "Roti tawar",
        "0",
        Some(3),
        "2",
        30,
        &[
            product_item("Tepung terigu protein tinggi", "1", "0"),
            product_item("Gula pasir", "0.06", "0"),
            product_item("Mentega tawar", "0.05", "0"),
            product_item("Telur ayam", "0.02", "0"),
            product_item("Ragi instan", "10", "0"),
            product_item("Garam", "10", "0"),
            product_item("Susu UHT", "0.3", "0"),
        ],
    ),
    product(
        "Bolu keju",
        "0",
        Some(3),
        "1",
        45,
        &[
            product_item("Tepung terigu protein sedang", "0.5", "0"),
            product_item("Telur ayam", "1", "0"),
            product_item("Gula halus", "0.4", "0"),
            product_item("Mentega tawar", "0.2", "0"),
            product_item("Keju cheddar", "0.15", "0"),
            product_item("Baking powder", "5", "0"),
            product_item("Susu UHT", "0.2", "0"),
        ],
    ),
    product(
        "Brownies cokelat",
        "0",
        Some(7),
        "24",
        40,
        &[
            product_item("Tepung terigu protein sedang", "0.2", "0"),
            product_item("Cokelat compound", "0.3", "0"),
            product_item("Mentega tawar", "0.2", "0"),
            product_item("Telur ayam", "0.3", "0"),
            product_item("Gula halus", "0.25", "0"),
            product_item("Baking powder", "3", "0"),
        ],
    ),
];

const COFFEE: &[TemplateItem] = &[
    item("Biji kopi arabika", "kg", "3", Some(180)),
    item("Biji kopi robusta", "kg", "2", Some(180)),
    item("Susu UHT full cream", "l", "12", Some(120)),
    item("Susu kental manis", "pcs", "12", Some(365)),
    item("Gula aren cair", "l", "2", Some(90)),
    item("Gula pasir", "kg", "5", Some(730)),
    item("Sirup vanila", "l", "2", Some(365)),
    item("Sirup karamel", "l", "2", Some(365)),
    item("Bubuk matcha", "kg", "1", Some(180)),
    item("Bubuk cokelat", "kg", "2", Some(365)),
    item("Teh celup", "pcs", "200", Some(365)),
    item("Es batu", "kg", "10", None),
    item("Cup plastik 16 oz", "pcs", "500", None),
    item("Lid cup", "pcs", "500", None),
    item("Sedotan", "pcs", "500", None),
    item("Tisu", "pcs", "20", None),
];

const COFFEE_PRODUCTS: &[TemplateProduct] = &[
    product(
        "Es kopi susu",
        "0",
        None,
        "1",
        5,
        &[
            product_item("Biji kopi robusta", "0.018", "0"),
            product_item("Susu UHT full cream", "0.2", "0"),
            product_item("Gula aren cair", "0.03", "0"),
            product_item("Es batu", "0.15", "0"),
            product_item("Cup plastik 16 oz", "1", "0"),
            product_item("Lid cup", "1", "0"),
            product_item("Sedotan", "1", "0"),
        ],
    ),
    product(
        "Matcha latte es",
        "0",
        None,
        "1",
        5,
        &[
            product_item("Bubuk matcha", "0.005", "0"),
            product_item("Susu UHT full cream", "0.25", "0"),
            product_item("Gula aren cair", "0.03", "0"),
            product_item("Es batu", "0.15", "0"),
            product_item("Cup plastik 16 oz", "1", "0"),
            product_item("Lid cup", "1", "0"),
        ],
    ),
    product(
        "Es cokelat",
        "0",
        None,
        "1",
        5,
        &[
            product_item("Bubuk cokelat", "0.03", "0"),
            product_item("Susu UHT full cream", "0.2", "0"),
            product_item("Gula aren cair", "0.02", "0"),
            product_item("Es batu", "0.15", "0"),
            product_item("Cup plastik 16 oz", "1", "0"),
            product_item("Lid cup", "1", "0"),
        ],
    ),
];

const RICE_BOWL: &[TemplateItem] = &[
    item("Beras", "kg", "25", Some(365)),
    item("Ayam fillet", "kg", "10", Some(7)),
    item("Daging sapi giling", "kg", "5", Some(7)),
    item("Telur ayam", "kg", "5", Some(21)),
    item("Tahu", "pcs", "30", Some(3)),
    item("Tempe", "pcs", "30", Some(3)),
    item("Bawang merah", "kg", "3", Some(30)),
    item("Bawang putih", "kg", "3", Some(60)),
    item("Cabai merah", "kg", "2", Some(7)),
    item("Cabai rawit", "kg", "1", Some(7)),
    item("Kecap manis", "l", "3", Some(365)),
    item("Saus tiram", "l", "2", Some(365)),
    item("Minyak goreng", "l", "10", Some(180)),
    item("Garam", "kg", "2", Some(1825)),
    item("Gula pasir", "kg", "3", Some(730)),
    item("Merica", "kg", "1", Some(730)),
    item("Daun bawang", "kg", "1", Some(7)),
    item("Sawi", "kg", "3", Some(5)),
    item("Box makanan", "pcs", "200", None),
    item("Sendok plastik", "pcs", "200", None),
];

const RICE_BOWL_PRODUCTS: &[TemplateProduct] = &[
    product(
        "Nasi ayam goreng",
        "0",
        Some(1),
        "1",
        15,
        &[
            product_item("Beras", "0.15", "0"),
            product_item("Ayam fillet", "0.1", "5"),
            product_item("Bawang putih", "0.01", "0"),
            product_item("Garam", "0.003", "0"),
            product_item("Minyak goreng", "0.02", "0"),
            product_item("Box makanan", "1", "0"),
            product_item("Sendok plastik", "1", "0"),
        ],
    ),
    product(
        "Nasi sapi teriyaki",
        "0",
        Some(1),
        "1",
        15,
        &[
            product_item("Beras", "0.15", "0"),
            product_item("Daging sapi giling", "0.08", "0"),
            product_item("Kecap manis", "0.02", "0"),
            product_item("Saus tiram", "0.01", "0"),
            product_item("Gula pasir", "0.005", "0"),
            product_item("Minyak goreng", "0.015", "0"),
            product_item("Box makanan", "1", "0"),
            product_item("Sendok plastik", "1", "0"),
        ],
    ),
    product(
        "Nasi tahu tempe",
        "0",
        Some(1),
        "1",
        12,
        &[
            product_item("Beras", "0.15", "0"),
            product_item("Tahu", "2", "0"),
            product_item("Tempe", "2", "0"),
            product_item("Cabai merah", "0.02", "5"),
            product_item("Bawang merah", "0.02", "3"),
            product_item("Minyak goreng", "0.02", "0"),
            product_item("Box makanan", "1", "0"),
            product_item("Sendok plastik", "1", "0"),
        ],
    ),
];

const DESSERT: &[TemplateItem] = &[
    item("Susu UHT", "l", "6", Some(120)),
    item("Krim kental manis", "pcs", "12", Some(365)),
    item("Gula pasir", "kg", "3", Some(730)),
    item("Agar-agar bubuk", "pcs", "20", Some(730)),
    item("Bubuk jelly", "pcs", "20", Some(730)),
    item("Buah kalengan", "pcs", "12", Some(365)),
    item("Nata de coco", "pcs", "12", Some(365)),
    item("Sirup markisa", "l", "2", Some(365)),
    item("Es batu", "kg", "10", None),
    item("Cup dessert", "pcs", "300", None),
    item("Sendok plastik", "pcs", "300", None),
    item("Pewarna makanan", "ml", "200", Some(730)),
    item("Vanili", "g", "100", Some(730)),
    item("Tepung maizena", "kg", "1", Some(365)),
    item("Telur ayam", "kg", "3", Some(21)),
    item("Keju cheddar", "kg", "1", Some(180)),
];

const DESSERT_PRODUCTS: &[TemplateProduct] = &[
    product(
        "Puding agar mangkok",
        "0",
        Some(3),
        "12",
        25,
        &[
            product_item("Agar-agar bubuk", "1", "0"),
            product_item("Gula pasir", "0.2", "0"),
            product_item("Susu UHT", "0.5", "0"),
            product_item("Cup dessert", "12", "0"),
            product_item("Sendok plastik", "12", "0"),
        ],
    ),
    product(
        "Es buah",
        "0",
        Some(1),
        "1",
        8,
        &[
            product_item("Buah kalengan", "1", "10"),
            product_item("Nata de coco", "1", "0"),
            product_item("Sirup markisa", "0.05", "0"),
            product_item("Es batu", "0.15", "0"),
            product_item("Cup dessert", "1", "0"),
            product_item("Sendok plastik", "1", "0"),
        ],
    ),
    product(
        "Cheese pudding",
        "0",
        Some(3),
        "12",
        30,
        &[
            product_item("Krim kental manis", "1", "0"),
            product_item("Susu UHT", "0.4", "0"),
            product_item("Gula pasir", "0.1", "0"),
            product_item("Keju cheddar", "0.05", "0"),
            product_item("Tepung maizena", "0.02", "0"),
            product_item("Cup dessert", "12", "0"),
            product_item("Sendok plastik", "12", "0"),
        ],
    ),
];

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "bakery",
        items: BAKERY,
        products: BAKERY_PRODUCTS,
    },
    Template {
        id: "coffee",
        items: COFFEE,
        products: COFFEE_PRODUCTS,
    },
    Template {
        id: "rice_bowl",
        items: RICE_BOWL,
        products: RICE_BOWL_PRODUCTS,
    },
    Template {
        id: "dessert",
        items: DESSERT,
        products: DESSERT_PRODUCTS,
    },
];

#[derive(Debug, Serialize)]
pub struct TemplateItemView {
    pub name: &'static str,
    pub unit: &'static str,
    pub min_stock: &'static str,
    pub shelf_life_days: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct TemplateView {
    pub id: &'static str,
    pub item_count: usize,
    pub product_count: usize,
    pub items: Vec<TemplateItemView>,
}

pub async fn list_templates(_guard: Require<CanReadMaterials>) -> Json<Vec<TemplateView>> {
    Json(
        TEMPLATES
            .iter()
            .map(|template| TemplateView {
                id: template.id,
                item_count: template.items.len(),
                product_count: template.products.len(),
                items: template
                    .items
                    .iter()
                    .map(|item| TemplateItemView {
                        name: item.name,
                        unit: item.unit,
                        min_stock: item.min_stock,
                        shelf_life_days: item.shelf_life_days,
                    })
                    .collect(),
            })
            .collect(),
    )
}

/// Unit ids by code. Only system units and the tenant's own are visible.
pub async fn unit_ids(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<std::collections::HashMap<String, Uuid>, ApiError> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("select id, code from units where tenant_id is null or tenant_id = $1")
            .bind(tenant_id)
            .fetch_all(&mut **tx)
            .await?;

    Ok(rows.into_iter().map(|(id, code)| (code, id)).collect())
}

/// Lower-cased names already in use, so an import can skip instead of fail.
pub async fn existing_names(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<HashSet<String>, ApiError> {
    let rows: Vec<(String,)> =
        sqlx::query_as("select lower(name) from materials where tenant_id = $1")
            .bind(tenant_id)
            .fetch_all(&mut **tx)
            .await?;

    Ok(rows.into_iter().map(|(name,)| name).collect())
}

/// Ids of the template's own materials, keyed by lower-cased name. A row is
/// found both when this apply created it and when the tenant already had it,
/// which is what lets a recipe item always resolve.
async fn template_material_ids(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    template: &Template,
) -> Result<HashMap<String, Uuid>, ApiError> {
    let names: Vec<String> = template
        .items
        .iter()
        .map(|item| item.name.to_lowercase())
        .collect();

    let rows: Vec<(String, Uuid)> = sqlx::query_as(
        "select lower(name), id from materials
         where tenant_id = $1 and lower(name) = any($2)",
    )
    .bind(tenant_id)
    .bind(&names)
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows.into_iter().collect())
}

/// Writes the template's products and the first version of each recipe.
///
/// A product the tenant already has is skipped, and its recipe with it: a
/// template is a starting point and must never overwrite a recipe the user may
/// have edited. The whole apply is one transaction, so a failure leaves the
/// tenant exactly as it was.
async fn apply_products(
    tx: &mut Transaction<'_, Postgres>,
    auth: &crate::auth::extract::AuthContext,
    template: &Template,
    material_ids: &HashMap<String, Uuid>,
) -> Result<(i64, i64, i64), ApiError> {
    let taken: HashSet<String> =
        sqlx::query_as("select lower(name) from products where tenant_id = $1")
            .bind(auth.tenant_id)
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .map(|(name,)| name)
            .collect();

    let mut products = 0_i64;
    let mut recipes = 0_i64;
    let mut skipped = 0_i64;

    for product in template.products {
        if taken.contains(&product.name.to_lowercase()) {
            skipped += 1;
            continue;
        }

        let yield_qty = Decimal::from_str(product.yield_qty)
            .map_err(|_| ApiError::Internal(anyhow::anyhow!("invalid template yield")))?;

        if yield_qty <= Decimal::ZERO {
            return Err(ApiError::Internal(anyhow::anyhow!(
                "template yield must be positive"
            )));
        }

        // Every item has to resolve, or the recipe would name a material the
        // tenant does not have. The template's own material list guarantees it,
        // so this is a guard against a template that drifted, not a path a
        // user can reach.
        let mut items = Vec::with_capacity(product.items.len());

        for item in product.items {
            let material_id = material_ids
                .get(&item.material.to_lowercase())
                .ok_or_else(|| {
                    ApiError::Internal(anyhow::anyhow!(
                        "template material {} is missing",
                        item.material
                    ))
                })?;

            items.push((
                *material_id,
                Decimal::from_str(item.qty).unwrap_or_default(),
                Decimal::from_str(item.waste_pct).unwrap_or_default(),
            ));
        }

        let sell_price = Decimal::from_str(product.sell_price).unwrap_or_default();

        let (product_id,): (Uuid,) = sqlx::query_as(
            "insert into products (tenant_id, name, sell_price, shelf_life_days)
             values ($1, $2, $3, $4) returning id",
        )
        .bind(auth.tenant_id)
        .bind(product.name)
        .bind(sell_price)
        .bind(product.shelf_life_days)
        .fetch_one(&mut **tx)
        .await?;

        let (recipe_id,): (Uuid,) = sqlx::query_as(
            "insert into recipes
                 (tenant_id, product_id, version, yield_qty, labor_minutes, created_by)
             values ($1, $2, 1, $3, $4, $5) returning id",
        )
        .bind(auth.tenant_id)
        .bind(product_id)
        .bind(yield_qty)
        .bind(product.labor_minutes)
        .bind(auth.user_id)
        .fetch_one(&mut **tx)
        .await?;

        for (material_id, qty, waste_pct) in &items {
            sqlx::query(
                "insert into recipe_items (recipe_id, material_id, qty, waste_pct)
                 values ($1, $2, $3, $4)",
            )
            .bind(recipe_id)
            .bind(material_id)
            .bind(qty)
            .bind(waste_pct)
            .execute(&mut **tx)
            .await?;
        }

        // The product produces with the version just written.
        sqlx::query("update products set current_recipe_id = $1, updated_at = now() where id = $2")
            .bind(recipe_id)
            .bind(product_id)
            .execute(&mut **tx)
            .await?;

        products += 1;
        recipes += 1;
    }

    Ok((products, recipes, skipped))
}

pub async fn apply_template(
    State(state): State<AppState>,
    guard: Require<CanWriteMaterials>,
    Path(id): Path<String>,
) -> Result<Json<ImportResult>, ApiError> {
    let auth = guard.auth;
    let template = TEMPLATES
        .iter()
        .find(|template| template.id == id)
        .ok_or(ApiError::NotFound("template_not_found"))?;

    let db = state.db()?;
    let mut tx = nata_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let units = unit_ids(&mut tx, auth.tenant_id).await?;
    let mut taken = existing_names(&mut tx, auth.tenant_id).await?;

    let mut created = 0_i64;
    let mut skipped = 0_i64;

    for item in template.items {
        let key = item.name.to_lowercase();

        if taken.contains(&key) {
            skipped += 1;
            continue;
        }

        let unit_id = *units
            .get(item.unit)
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("missing unit {}", item.unit)))?;

        sqlx::query(
            "insert into materials (tenant_id, name, base_unit_id, min_stock, shelf_life_days)
             values ($1, $2, $3, $4, $5)",
        )
        .bind(auth.tenant_id)
        .bind(item.name)
        .bind(unit_id)
        .bind(Decimal::from_str(item.min_stock).unwrap_or_default())
        .bind(item.shelf_life_days)
        .execute(&mut *tx)
        .await?;

        taken.insert(key);
        created += 1;
    }

    let material_ids = template_material_ids(&mut tx, auth.tenant_id, template).await?;

    // Onboarding is available on every plan with a row cap, so the template
    // path deliberately does not check the plan's recipe ceiling. The material
    // import above already works the same way.
    let (products, recipes, skipped_products) =
        apply_products(&mut tx, &auth, template, &material_ids).await?;

    tx.commit().await?;

    Ok(Json(ImportResult {
        created,
        skipped: skipped + skipped_products,
        errors: Vec::new(),
        products,
        recipes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::collections::HashSet;
    use std::str::FromStr;

    /// A recipe item may only name a material the same template stands up.
    /// `apply_products` treats a miss as an internal error, so a template that
    /// drifted would fail the whole apply instead of writing a broken BOM.
    #[test]
    fn every_recipe_item_names_a_material_of_its_own_template() {
        for template in TEMPLATES {
            let materials: HashSet<String> = template
                .items
                .iter()
                .map(|item| item.name.to_lowercase())
                .collect();

            for product in template.products {
                assert!(
                    !product.items.is_empty(),
                    "{} has an empty recipe",
                    product.name
                );

                for item in product.items {
                    assert!(
                        materials.contains(&item.material.to_lowercase()),
                        "{}.{} names {}, which the template does not bring",
                        product.name,
                        item.material,
                        item.material
                    );
                }
            }
        }
    }

    /// Quantities, waste and yields are parsed with `Decimal::from_str`, so a
    /// typo in a literal fails here rather than at apply time.
    #[test]
    fn every_template_number_parses_and_stays_in_range() {
        for template in TEMPLATES {
            for item in template.items {
                assert!(
                    Decimal::from_str(item.min_stock).is_ok(),
                    "{} has an unparsable minimum stock",
                    item.name
                );
            }

            for product in template.products {
                let yield_qty = Decimal::from_str(product.yield_qty)
                    .unwrap_or_else(|_| panic!("{} has an unparsable yield", product.name));
                assert!(yield_qty > Decimal::ZERO, "{} yields nothing", product.name);

                assert!(
                    Decimal::from_str(product.sell_price).is_ok(),
                    "{} has an unparsable price",
                    product.name
                );

                for item in product.items {
                    let qty = Decimal::from_str(item.qty).unwrap_or_else(|_| {
                        panic!("{}.{} has an unparsable qty", product.name, item.material)
                    });
                    assert!(
                        qty > Decimal::ZERO,
                        "{}.{} uses none",
                        product.name,
                        item.material
                    );

                    let waste = Decimal::from_str(item.waste_pct).unwrap_or_else(|_| {
                        panic!("{}.{} has unparsable waste", product.name, item.material)
                    });
                    assert!(
                        (Decimal::ZERO..=Decimal::from(100)).contains(&waste),
                        "{}.{} waste is out of range",
                        product.name,
                        item.material
                    );
                }
            }
        }
    }

    /// A repeated name would make the apply skip a row the author wrote on
    /// purpose, and both products and materials are matched case-insensitively.
    #[test]
    fn names_are_unique_within_each_template() {
        for template in TEMPLATES {
            let mut materials = HashSet::new();
            for item in template.items {
                assert!(
                    materials.insert(item.name.to_lowercase()),
                    "{} repeats the material {}",
                    template.id,
                    item.name
                );
            }

            let mut products = HashSet::new();
            for product in template.products {
                assert!(
                    products.insert(product.name.to_lowercase()),
                    "{} repeats the product {}",
                    template.id,
                    product.name
                );
            }
        }
    }
}
