# SMTPSpammer

A command-line tool for sending bulk email via [Proton Mail SMTP](https://proton.me/support/smtp-submission).

## Usage

### Store your Proton Mail SMTP credentials

The key is stored as `email:smtp_token`, where `smtp_token` is the dedicated SMTP token you generate inside your Proton Mail account settings (not your account password).

```
smtpspammer key new your@proton.me:your_smtp_token
```

### Retrieve the stored key

```
smtpspammer key get
```

### Send bulk email

```
smtpspammer send <count> <recipient> "<subject>" "<body>"
```

Example – send 100 emails:

```
smtpspammer send 100 ari@aricummings.com "hi" "boo"
```

## SMTP details

| Setting | Value |
|---------|-------|
| Host | `smtp.protonmail.ch` |
| Port | `587` |
| Encryption | STARTTLS |
| Auth | PLAIN / LOGIN |

## Storage

The key is saved as plain JSON in your platform's config directory:

| Platform | Path |
|----------|------|
| Windows | `%APPDATA%\smtpspammer\config.json` |
| macOS | `~/Library/Application Support/smtpspammer/config.json` |
| Linux | `~/.config/smtpspammer/config.json` |

> **Security note:** The key is stored as plaintext. Ensure the config file is readable only by your own user account.

## Building from source

```
cargo build --release
```

The compiled binary will be at `target/release/smtpspammer`.
