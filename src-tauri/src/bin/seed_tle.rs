//! Headless seeder: fetches TLE sets from CelesTrak and upserts them into the
//! shared dev database resolved by `core::db::resolve_db_path` (the same path
//! the Tauri app opens), regardless of where this binary is run from.
//!
//!     cargo run --bin seed_tle
//!
//! This binary lives outside the Tauri runtime; it is a one-shot CLI used to
//! validate Faz 2 end-to-end (network -> parse -> SGP4 -> az/el for ISS).

use chrono::Utc;
use skycomet_lib::core::db::{self, Database};
use skycomet_lib::core::location::Location;
use skycomet_lib::core::orbit::coordinates::teme_to_az_el;
use skycomet_lib::core::orbit::sgp4_engine::Propagator;
use skycomet_lib::core::tle::fetcher::{fetch_group, CelestrakGroup};
use skycomet_lib::core::tle::repo;

const ISS_NORAD: u32 = 25544;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = db::resolve_db_path(None)?;
    println!("opening db at {}", db_path.display());
    let db = Database::open(&db_path)?;

    let groups = [
        CelestrakGroup::Stations,
        CelestrakGroup::Amateur,
        CelestrakGroup::Weather,
    ];

    let mut total_records = 0;
    let mut total_skipped = 0;

    for group in groups {
        print!("fetching {:?} ... ", group);
        let outcome = match fetch_group(group).await {
            Ok(o) => o,
            Err(e) => {
                println!("FAILED: {e}");
                continue;
            }
        };
        let written = repo::upsert_many(&db, &outcome.records, &format!("celestrak:{:?}", group))?;
        println!(
            "{} records, {} skipped, {} written",
            outcome.records.len(),
            outcome.skipped.len(),
            written
        );
        total_records += outcome.records.len();
        total_skipped += outcome.skipped.len();
    }

    let count = repo::count(&db)?;
    println!("\nfetched: {total_records}, skipped: {total_skipped}, db total: {count}");

    // ISS sanity check
    println!("\n--- ISS (NORAD {ISS_NORAD}) az/el right now from Istanbul ---");
    match repo::get_by_norad(&db, ISS_NORAD)? {
        Some(record) => {
            let propagator = Propagator::from_tle(&record)?;
            let now = Utc::now();
            let state = propagator.propagate_at(now)?;
            let observer = Location::new(41.0082, 28.9784, 35.0)?;
            let az_el = teme_to_az_el(state.position_km, now, &observer)?;
            println!("epoch     : {}", record.epoch);
            println!("now       : {}", now);
            println!(
                "tle age   : {:.2} hours",
                (now - record.epoch).num_seconds() as f64 / 3600.0
            );
            println!("azimuth   : {:.3}°", az_el.azimuth_deg);
            println!("elevation : {:.3}°", az_el.elevation_deg);
            println!("range     : {:.1} km", az_el.range_km);
            println!("\nCompare against https://www.n2yo.com/?s=25544 (set location to 41.0082, 28.9784).");
            println!("Acceptable: az/el within 0.5° of N2YO reading at the same instant.");
        }
        None => {
            println!("ISS not in DB — stations group fetch may have failed.");
        }
    }

    Ok(())
}
