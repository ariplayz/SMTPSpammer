use clap::{Parser, Subcommand};
use dirs::config_dir;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(name = "smtpspammer", about = "Bulk email sender via Proton Mail SMTP")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage the stored Proton Mail SMTP key
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Send bulk emails
    ///
    /// Example: smtpspammer send 100 ari@aricummings.com "hi" "boo"
    Send {
        /// Number of emails to send
        count: u32,
        /// Recipient email address
        recipient: String,
        /// Email subject
        subject: String,
        /// Email body
        body: String,
    },
}

#[derive(Subcommand)]
enum KeyAction {
    /// Store a new Proton Mail SMTP key (format: your@proton.me:smtp_token)
    New {
        /// SMTP credentials in the format email:smtp_token
        key: String,
    },
    /// Print the currently stored SMTP key
    Get,
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    key: Option<String>,
}

fn config_path() -> PathBuf {
    let mut path = config_dir().unwrap_or_else(|| {
        eprintln!("error: could not locate a config directory on this platform");
        process::exit(1);
    });
    path.push("smtpspammer");
    path.push("config.json");
    path
}

fn load_config() -> Config {
    let path = config_path();
    if !path.exists() {
        return Config::default();
    }
    let data = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("error: failed to read config file: {e}");
        process::exit(1);
    });
    serde_json::from_str(&data).unwrap_or_else(|e| {
        eprintln!("error: failed to parse config file: {e}");
        process::exit(1);
    })
}

fn save_config(config: &Config) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("error: failed to create config directory: {e}");
            process::exit(1);
        });
    }
    let data = serde_json::to_string_pretty(config).unwrap_or_else(|e| {
        eprintln!("error: failed to serialize config: {e}");
        process::exit(1);
    });
    fs::write(&path, data).unwrap_or_else(|e| {
        eprintln!("error: failed to write config file: {e}");
        process::exit(1);
    });
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Key { action } => match action {
            KeyAction::New { key } => {
                let mut config = load_config();
                config.key = Some(key);
                save_config(&config);
                println!("SMTP key stored successfully.");
            }
            KeyAction::Get => {
                let config = load_config();
                match config.key {
                    Some(key) => println!("{key}"),
                    None => {
                        eprintln!(
                            "No SMTP key stored. \
                             Run 'smtpspammer key new <email:smtp_token>' to store one."
                        );
                        process::exit(1);
                    }
                }
            }
        },

        Commands::Send {
            count,
            recipient,
            subject,
            body,
        } => {
            let config = load_config();
            let key = config.key.unwrap_or_else(|| {
                eprintln!(
                    "No SMTP key stored. \
                     Run 'smtpspammer key new <email:smtp_token>' to store one."
                );
                process::exit(1);
            });

            // The key is stored as "email:smtp_token"; split on the first colon only
            // so that the token itself may contain colons.
            let (username, smtp_token) = key.split_once(':').unwrap_or_else(|| {
                eprintln!(
                    "Invalid key format. \
                     Expected 'your@proton.me:smtp_token'."
                );
                process::exit(1);
            });

            let from_mailbox: lettre::message::Mailbox =
                username.parse().unwrap_or_else(|e| {
                    eprintln!("error: invalid sender address '{username}': {e}");
                    process::exit(1);
                });
            let to_mailbox: lettre::message::Mailbox =
                recipient.parse().unwrap_or_else(|e| {
                    eprintln!("error: invalid recipient address '{recipient}': {e}");
                    process::exit(1);
                });

            let creds = Credentials::new(username.to_string(), smtp_token.to_string());

            // Proton Mail SMTP: smtp.protonmail.ch, port 587, STARTTLS
            let mailer = SmtpTransport::starttls_relay("smtp.protonmail.ch")
                .unwrap_or_else(|e| {
                    eprintln!("error: failed to initialize SMTP transport: {e}");
                    process::exit(1);
                })
                .port(587)
                .credentials(creds)
                .build();

            let sent = AtomicU32::new(0);
            const CONCURRENCY: usize = 30;

            // pending holds the 1-based indices of emails still to be sent
            let mut pending: Vec<u32> = (1..=count).collect();
            let mut round = 0u32;

            while !pending.is_empty() {
                round += 1;
                if round > 1 {
                    println!("\nRetrying {} failed email(s)...", pending.len());
                }

                let next_failed: Mutex<Vec<u32>> = Mutex::new(Vec::new());

                thread::scope(|s| {
                    let mut handles = Vec::with_capacity(CONCURRENCY);

                    for i in pending.iter().copied() {
                        let from = from_mailbox.clone();
                        let to = to_mailbox.clone();
                        let subj = &subject;
                        let bod = body.clone();
                        let sent_ref = &sent;
                        let next_failed_ref = &next_failed;
                        let mailer_ref = &mailer;

                        handles.push(s.spawn(move || {
                            let ts = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|d| d.as_nanos())
                                .unwrap_or(0);
                            let msg_id =
                                format!("<{i}.{ts}@smtpspammer.local>");
                            let email = Message::builder()
                                .from(from)
                                .to(to)
                                .subject(subj)
                                .message_id(Some(msg_id))
                                .body(bod)
                                .unwrap_or_else(|e| {
                                    eprintln!("error: failed to build email message: {e}");
                                    process::exit(1);
                                });

                            match mailer_ref.send(&email) {
                                Ok(_) => {
                                    sent_ref.fetch_add(1, Ordering::Relaxed);
                                    println!("[{i}/{count}] Sent successfully.");
                                }
                                Err(e) => {
                                    eprintln!("[{i}/{count}] Failed: {e}");
                                    next_failed_ref
                                        .lock()
                                        .unwrap_or_else(|e| e.into_inner())
                                        .push(i);
                                }
                            }
                        }));

                        if handles.len() == CONCURRENCY {
                            for h in handles.drain(..) {
                                let _ = h.join();
                            }
                        }
                    }

                    for h in handles {
                        let _ = h.join();
                    }
                });

                pending = next_failed.into_inner().unwrap_or_else(|e| e.into_inner());
            }

            println!(
                "\nFinished: {} sent in {} round(s).",
                sent.load(Ordering::Relaxed),
                round
            );
        }
    }
}
