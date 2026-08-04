use std::str::FromStr as _;

use croner::{Cron, Direction};
use jiff::Zoned;

fn main() {
    // Parse cron expression
    let cron = Cron::from_str("18 * * * 5").expect("Couldn't parse cron string");

    // Find the next occurrence in Europe/Stockholm.
    // The return type follows the argument type, so this is a `jiff::Zoned`.
    let now_stockholm = Zoned::now().in_tz("Europe/Stockholm").unwrap();
    let next_stockholm: Zoned = cron.find_next_occurrence(&now_stockholm, false).unwrap();

    println!("Time in Europe/Stockholm is: {now_stockholm}");
    println!(
        "Pattern \"{}\" will match next time at (Europe/Stockholm): {next_stockholm}",
        cron.pattern
    );

    // The iterators work the same way.
    println!("The five following matches are:");
    for occurrence in cron
        .iter_from(now_stockholm, Direction::Forward)
        .skip(1)
        .take(5)
    {
        println!("  {occurrence}");
    }
}
