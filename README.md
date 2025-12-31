# pjhoy - Pirkanmaan Jätehuolto Oy utility

A command line tool for accessing trash pickup schedules from Pirkanmaan Jätehuolto Oy's extranet service.

## Features

### Login and Session Management

Command `pjhoy login` gets username and password from user
configuration (usually `~/.config/pjoy/credentials`) and does cookie
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

A calendar file (.ics) is maintained with latest pickup dates
appended. Trash type (`tariff.id` in JSON) and next pickup date
(`ASTNextDate` in JSON) together can be used for a unique event
identifier.

### Systemd Integration

Designed to work with systemd timers for regular updates.
