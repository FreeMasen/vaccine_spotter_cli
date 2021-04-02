use std::{collections::HashMap, path::PathBuf};
use chrono::{Date, DateTime, Local};
use structopt::StructOpt;

use serde::Deserialize;

type R<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(StructOpt, Debug)]
struct Args {
    #[structopt(short, long)]
    /// The path to a json file containing an array of strings representing
    /// the target zip codes. If not provided all zipcodes will be considered
    zips_path: Option<PathBuf>,
    #[structopt(short, long)]
    /// the 2 digit state code to use to get current appointments
    state: String,
    #[structopt(short, long)]
    /// The email address to send alerts from
    from_email: Option<String>,
    #[structopt(short, long)]
    /// The email address to send alerts to
    to_email: Option<String>,
}

#[tokio::main]
async fn main() -> R<()> {
    pretty_env_logger::init();
    let args = Args::from_args();
    log::debug!("starting with args: {:?}", args);
    let mut current_info: HashMap<u64, Vec<Appointment>> = HashMap::new();
    let zips = fetch_considered_zips(&args.zips_path);
    loop {
        if let Ok(res) = reqwest::get(&format!(
            "https://www.vaccinespotter.org/api/v0/states/{}.json",
            args.state.to_uppercase()
        ))
        .await
        {
            log::info!("requesting new appointments");
            let res: Response = match res.json().await {
                Ok(res) => res,
                Err(e) => {
                    log::error!("Failed to request new appointments: {}", e);
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
            };
            log::info!("new appoints received");
            report_locations(
                &res.features,
                &current_info,
                &zips,
                &args.from_email,
                &args.to_email,
            );
            current_info = res
                .features
                .into_iter()
                .map(|f| {
                    (
                        f.properties.id,
                        f.properties.appointments.unwrap_or_default(),
                    )
                })
                .collect();
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

fn report_locations(
    locations: &[Feature],
    current_info: &HashMap<u64, Vec<Appointment>>,
    zips: &[String],
    from_email: &Option<String>,
    to_email: &Option<String>,
) {
    if let (Some(from_email), Some(to_email)) = (from_email, to_email) {
        if let Err(e) = email_locations(locations, current_info, zips, from_email, to_email) {
            eprintln!(
                "Failed to send email from {} to {}: {}",
                from_email, to_email, e
            );
        }
    } else {
        print_locations(locations, current_info, zips)
    }
}

#[cfg(not(feature = "email-notifications"))]
fn email_locations(
    locations: &[Feature],
    current_info: &HashMap<u64, Vec<Appointment>>,
    zips: &[String],
    _from_email: &str,
    _to_email: &str,
) -> R<()> {
    print_locations(locations, current_info, zips);
    Ok(())
}
fn print_locations(
    locations: &[Feature],
    current_info: &HashMap<u64, Vec<Appointment>>,
    zips: &[String],
) {
    let mut printed_preamble = false;
    for entry in locations {
        if let Some(appointments) = &entry.properties.appointments {
            if let Some(info) = current_info.get(&entry.properties.id) {
                if contains_new_appts(appointments, info) {
                    if let Some(zip) = &entry.properties.postal_code {
                        if zips.is_empty() || zips.contains(zip) {
                            if !printed_preamble {
                                println!("{}", "=".repeat(10));
                                println!("Report as of {}", chrono::Local::now());
                                println!("{}", "=".repeat(10));
                                printed_preamble = true
                            }
                            print_location(&entry.properties);
                        }
                    }
                }
            } else if !appointments.is_empty() {
                if let Some(zip) = &entry.properties.postal_code {
                    if zips.is_empty() || zips.contains(zip) {
                        if !printed_preamble {
                            println!("{}", "=".repeat(10));
                            println!("Report as of {}", chrono::Local::now());
                            println!("{}", "=".repeat(10));
                            printed_preamble = true
                        }
                        print_location(&entry.properties);
                    }
                }
            }
        }
    }
}

fn contains_new_appts(new: &[Appointment], old: &[Appointment]) -> bool {
    for appt in new {
        if !old.contains(appt) {
            return true
        }
    }
    false
}

fn print_location(props: &Properties) {
    println!("{}", "+".repeat(10));
    println!("{}", props);
    println!("{}", "+".repeat(10));
}

#[cfg(feature = "email-notifications")]
fn email_locations(
    locations: &[Feature],
    current_info: &HashMap<u64, Vec<Appointment>>,
    zips: &[String],
    from_email: &str,
    to_email: &str,
) -> R<()> {
    use lettre::{Message, SmtpTransport, Transport};
    let mut body = format!(
        "{}\nReport as of {}\n{}\n\n",
        "=".repeat(10),
        chrono::Local::now(),
        "=".repeat(10),
    );
    let mut send = false;
    for entry in locations {
        if let Some(appointments) = &entry.properties.appointments {
            if let Some(info) = current_info.get(&entry.properties.id) {
                if contains_new_appts(appointments, info) {
                    if let Some(zip) = &entry.properties.postal_code {
                        if zips.is_empty() || zips.contains(zip) {
                            send = true;
                            body.push_str(&format!(
                                "{}\n{}\n{}\n",
                                "+".repeat(10),
                                &entry.properties,
                                "+".repeat(10)
                            ))
                        }
                    }
                }
            } else if !appointments.is_empty() {
                if let Some(zip) = &entry.properties.postal_code {
                    if zips.is_empty() || zips.contains(zip) {
                        send = true;
                        body.push_str(&format!(
                            "{}\n{}\n{}\n",
                            "+".repeat(10),
                            &entry.properties,
                            "+".repeat(10)
                        ))
                    }
                }
            }
        }
    }
    if !send {
        return Ok(());
    }
    let email = Message::builder()
        .to(from_email.parse()?)
        .to(to_email.parse()?)
        .subject("New Vaccine Appointments")
        .body(body)?;

    // Open a local connection on port 25
    let mailer = SmtpTransport::unencrypted_localhost();
    // Send the email
    mailer.send(&email)?;

    Ok(())
}

fn fetch_considered_zips(path: &Option<PathBuf>) -> Vec<String> {
    if let Some(path) = path {
        let s = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("failed to read zips.json to string: {}", e);
                return Vec::new();
            }
        };
        serde_json::from_str(&s).unwrap_or_default()
    } else {
        Vec::new()
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Response {
    features: Vec<Feature>,
}

#[derive(Clone, Debug, Deserialize)]
struct Feature {
    properties: Properties,
}

#[derive(Clone, Debug, Deserialize)]
struct Properties {
    id: u64,
    url: Option<String>,
    city: Option<String>,
    state: Option<String>,
    address: Option<String>,
    name: Option<String>,
    provider: Option<String>,
    postal_code: Option<String>,
    carries_vaccine: Option<bool>,
    appointments_available: Option<bool>,
    appointments_available_all_doses: Option<bool>,
    appointments_available_2nd_dose_only: Option<bool>,
    appointments: Option<Vec<Appointment>>,
}

impl std::fmt::Display for Properties {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(
            f,
            "{}-{}",
            string_or_question(&self.provider),
            string_or_question(&self.name)
        )?;
        writeln!(f, "{}", string_or_question(&self.url))?;
        writeln!(f, "{}", string_or_question(&self.address))?;
        writeln!(
            f,
            "{}, {} {}",
            string_or_question(&self.city),
            string_or_question(&self.state),
            string_or_question(&self.postal_code)
        )?;
        if let Some(apts) = &self.appointments {
            let sorted: HashMap<Date<Local>, Vec<DateTime<Local>>> = apts.iter().fold(std::collections::HashMap::new(), |mut acc, apt| {
                acc.entry(apt.time.date())
                    .and_modify(|v| v.push(apt.time))
                    .or_insert(vec![apt.time]);
                acc
            });
            let mut keys: Vec<Date<Local>> = sorted.keys().cloned().collect();
            keys.sort();
            for key in keys {
                write!(f, "{}", key.format("%m/%d/%Y: "))?;
                for (i, time) in sorted[&key].iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", time.format("%I:%M%P"))?;
                }
                writeln!(f, "")?;
            }
        }
        writeln!(f, "")
    }
}

fn string_or_question(o: &Option<String>) -> &str {
    if let Some(o) = o {
        o
    } else {
        "??"
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Appointment {
    time: DateTime<Local>,
}

impl PartialEq<DateTime<Local>> for Appointment {
    fn eq(&self, other: &DateTime<Local>) -> bool {
        self.time == *other
    }
}
