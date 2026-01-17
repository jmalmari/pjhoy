# pjhoy - Pirkanmaan Jätehuolto Oy utility

A command line tool for accessing trash pickup schedules from Pirkanmaan Jätehuolto Oy's extranet service.

## Features

### Login and Session Management

Command `pjhoy login` gets username and password from user
configuration (usually `~/.config/pjhoy/config.toml`) and does cookie
saving HTTP Form POST to

    https://extranet.pjhoy.fi/pirkka/j_acegi_security_check?target=2

Form data, with example credentials, is

- `j_username=<customer number>`
- `j_password=<password>`
- `remember-me=false`

Cookies received are persisted. All other API calls use these session
cookies to gain authorized access.

Customer number is of form xx-yyyyyyy-zz where zz=00 is used for login
but zz=01, zz=02, etc. identifies specific billable services.

### Trash Schedule Fetching

A JSON can be retrieved from

    https://extranet.pjhoy.fi/pirkka/secure/get_services_by_customer_numbers.do

with query string attributes `customerNumbers[]=xx-yyyyyyy-zz`, repeated for each wanted service. The name is of course URL encoded to `customerNumbers%5B%5D`.

The fetched JSON contains next pickup times, among other information about trash services.

### ICS Calendar Generation

A calendar file (.ics) is maintained with latest pickup dates. The calendar uses product groups as event titles with appropriate emoji icons:

- 🗑️ Sekajäte (SEK)
- 🍃 Biojäte (BIO)
- 📦 Kartonki (KK)
- 🔄 Muovi (MU)
- 📄 Paperi (PP)
- 🔧 Metalli (ME)
- 🥃 Lasi (LA)
- ☣️ Vaarallinen jäte (VU)

The original service names are moved to event descriptions for additional context.

## Configuration

Create a configuration file at `~/.config/pjhoy/config.toml`:

```toml
username = "xx-yyyyyyy-zz"
password = "secret"

# Customer numbers - only the last two digits (zz) are needed
customer_numbers = ["00", "01", "02", "03", "05"]
```

## Usage

### Login

```bash
pjhoy login
```

### Fetch trash schedule and update calendar

```bash
pjhoy fetch
```

### Generate calendar only

```bash
pjhoy calendar
```
