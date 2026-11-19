# common-tools

Generic tools that expose useful information to agents, including deterministic date/time operations and calendar data.

## Tools

### `get_tweet`

Retrieves a public X/Twitter post from its URL using the FxEmbed API v2.

Required argument:

- `url`: a post URL on `x.com`, `twitter.com`, `fixupx.com` or `fxtwitter.com`.

The tool extracts the numeric post ID locally and requests `https://api.fxtwitter.com/2/status/{id}`. Query parameters (including tracking parameters) are ignored. It returns the API's JSON response unchanged, preserving the available post text, author, timestamps, engagement counts, quoted post and media metadata. The input URL is never fetched; only the fixed FxEmbed API host is contacted. No API token is required.

### `get_holidays`

Retrieves public holidays or official school holidays from [OpenHolidays API](https://www.openholidaysapi.org/).

Required arguments:

- `holiday_type`: `public` or `school`.
- `country_code`: ISO 3166-1 alpha-2 country code (for example, `ES`).
- `valid_from` and `valid_to`: inclusive date range in `YYYY-MM-DD` format. Maximum range: three years.

Optional arguments:

- `subdivision_code`: administrative subdivision (for example, `ES-CL`, Castile and León).
- `group_code`: school holiday group, where supported by the country.
- `language_code`: ISO 639-1 language code for names; defaults to `EN`.

The response includes the query metadata and the holiday records returned by the API, preserving fields such as start/end dates, holiday type, nationwide status, regional scope, subdivisions, groups and tags.

The API is public and does not require credentials. Its holiday data is published under the Open Database License (ODbL); retain attribution to OpenHolidays API when using or redistributing the data.

### Date and time tools

- `get_current_datetime`: current UTC timestamp and Unix timestamps.
- `get_timezone`: current local datetime for an IANA timezone.
- `convert_timezone`: convert a datetime between IANA timezones.
- `calculate_date_difference`: elapsed and calendar-day differences.
- `get_date_info`: weekday, ISO week, day of year, month length and leap-year information.
- `parse_datetime`: parse a date/time string.
- `sleep`: bounded asynchronous delay (0–30 seconds).

All tools are registered from their TOML definitions in `data/`; agents must explicitly allow the tools in their configuration.
