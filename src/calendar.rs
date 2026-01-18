use crate::models::TrashService;
use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use ics::properties::{Description, DtStart, Summary};
use ics::{escape_text, Event, ICalendar};

/// Product groups mapping with Finnish names and icons
const PRODUCT_GROUPS: &[(&str, &str, &str)] = &[
    ("SEK", "Sekajäte", "🗑️"),
    ("BIO", "Biojäte", "🍃"),
    ("KK", "Kartonki", "📦"),
    ("MU", "Muovi", "🔄"),
    ("PP", "Paperi", "📄"),
    ("ME", "Metalli", "🔧"),
    ("LA", "Lasi", "🥃"),
    ("VU", "Vaarallinen jäte", "☣️"),
];

pub fn generate_calendar(services: &[TrashService]) -> Result<ICalendar<'_>> {
    let mut calendar = ICalendar::new("2.0", "-//pjhoy//trash calendar//EN");

    for service in services {
        if let Ok(event) = generate_calendar_event(service) {
            calendar.add_event(event);
        }
    }

    Ok(calendar)
}

fn generate_calendar_event(service: &TrashService) -> Result<Event<'_>> {
    let Some(next_date) = &service.ASTNextDate else {
        return Err(anyhow::anyhow!("Service has no next pickup date"));
    };

    let dstamp =
        NaiveDate::parse_from_str(next_date, "%Y-%m-%d").context("Failed to parse date")?;
    let service_type_id = service.ASTTyyppi.unwrap_or(0);

    let uid = format!(
        "pjhoy_{}_{}_{}_{}",
        service.ASTAsnro, service_type_id, service.ASTPos, next_date
    );

    let event_date_str = dstamp.format("%Y%m%d").to_string();
    let mut event = Event::new(uid, Utc::now().format("%Y%m%dT%H%M%SZ").to_string());

    event.push(DtStart::new(event_date_str));

    let product_group_title = get_product_group_title(service);

    if let Some(title) = product_group_title {
        event.push(Summary::new(escape_text(title)));
    } else {
        event.push(Summary::new(escape_text(format!(
            "Jäte: {}",
            &service.ASTNimi
        ))));
    }

    // Build description with optional cost information
    let mut description_lines = Vec::new();
    description_lines.push(service.ASTNimi.clone());

    if let Some(cost) = service.ASTHinta {
        description_lines.push(format!("Hinta: {:.2} € (sis. ALV)", 1.255 * cost));
    }

    description_lines.push(format!("{} viikon välein", service.ASTVali));

    event.push(Description::new(escape_text(description_lines.join("\n"))));

    Ok(event)
}

fn get_product_group_title(service: &TrashService) -> Option<String> {
    let product_group = service
        .tariff
        .as_ref()
        .and_then(|tariff| tariff.productgroup.as_ref())?;

    for (code, finnish_name, icon) in PRODUCT_GROUPS {
        if code == &product_group {
            return Some(format!("{} {}", icon, finnish_name));
        }
    }
    Some(format!("📦 {}", product_group))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Tariff, TrashService};

    fn parse_ics_properties(event_str: &str) -> std::collections::HashMap<String, Vec<String>> {
        let mut properties: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        let mut current_key: Option<String> = None;

        for line in event_str.lines() {
            if line.is_empty() {
                continue;
            }

            if line.starts_with(' ') || line.starts_with('\t') {
                // Continuation line
                if let Some(key) = &current_key {
                    if let Some(values) = properties.get_mut(key) {
                        if let Some(last_value) = values.last_mut() {
                            last_value.push_str(&line[1..]);
                        }
                    }
                }
                continue;
            }

            if let Some((name, value)) = line.split_once(':') {
                properties
                    .entry(name.to_string())
                    .or_insert_with(Vec::new)
                    .push(value.to_string());
                current_key = Some(name.to_string());
            } else {
                current_key = None;
            }
        }
        properties
    }

    #[test]
    fn test_event_creation_with_timestamp() -> Result<()> {
        // Create a sample trash service
        let service = TrashService {
            ASTNextDate: Some("2023-12-25".to_string()),
            ASTNimi: "Test Trash Pickup".to_string(),
            ASTAsnro: "12345".to_string(),
            ASTPos: 1,
            ASTTyyppi: Some(1),
            ASTHinta: Some(10.50),
            ASTVali: "6".to_string(),
            tariff: None,
        };

        // Generate the event
        let event = generate_calendar_event(&service)?;

        // Convert event to string
        let event_str = event.to_string();

        // Parse to check properties
        let properties = parse_ics_properties(&event_str);

        assert_eq!(
            properties.get("UID"),
            Some(&vec!["pjhoy_12345_1_1_2023-12-25".to_string()])
        );
        assert_eq!(
            properties.get("DTSTART"),
            Some(&vec!["20231225".to_string()])
        );
        assert_eq!(
            properties.get("SUMMARY"),
            Some(&vec!["Jäte: Test Trash Pickup".to_string()])
        );

        // Check description content
        let desc = properties.get("DESCRIPTION").unwrap().first().unwrap();
        assert!(desc.contains("Test Trash Pickup"));
        assert!(desc.contains("Maksu: 13.18 € (sis. ALV)"));
        assert!(desc.contains("6 viikon välein"));

        if let Some(dtstamps) = properties.get("DTSTAMP") {
            assert!(
                !dtstamps.is_empty(),
                "DTSTAMP should have at least one entry"
            );
            assert!(
                dtstamps.iter().all(|s| s.contains('T')),
                "DTSTAMP must have time component"
            );
        } else {
            panic!("DTSTAMP property not found in event");
        }

        Ok(())
    }

    #[test]
    fn test_product_group_titles() -> Result<()> {
        // Test with SEK product group
        let sek_service = TrashService {
            ASTNextDate: Some("2023-12-25".to_string()),
            ASTNimi: "Sekajäte säiliö".to_string(),
            ASTAsnro: "12345".to_string(),
            ASTPos: 1,
            ASTTyyppi: Some(1),
            ASTHinta: Some(10.50),
            ASTVali: "6".to_string(),
            tariff: Some(Tariff {
                productgroup: Some("SEK".to_string()),
                name: Some("Sekajäte".to_string()),
            }),
        };

        let event = generate_calendar_event(&sek_service)?;
        let event_str = event.to_string();
        let properties = parse_ics_properties(&event_str);

        assert!(event_str.contains("SUMMARY:🗑️ Sekajäte"));

        let desc = properties.get("DESCRIPTION").unwrap().first().unwrap();
        assert!(desc.contains("Sekajäte säiliö"));
        assert!(desc.contains("Maksu: 13.18 € (sis. ALV)"));
        assert!(desc.contains("6 viikon välein"));

        Ok(())
    }
}
