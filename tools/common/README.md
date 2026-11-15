# common-tools

Generic tools that expose information available directly from the runtime environment, without calling external services.

## Tools

### `get_current_datetime`
Returns the current UTC date and time as an RFC 3339 timestamp, together with UTC timezone and Unix timestamps in seconds and milliseconds. Takes no arguments.

### `get_timezone`
Returns the current local datetime, UTC offset, and abbreviation for an IANA timezone. Optional `timezone`; defaults to `UTC`.

### `convert_timezone`
Converts a datetime between IANA timezones. Required: `datetime`, `from_timezone`, and `to_timezone`. Datetimes may be RFC 3339 (with an explicit offset) or local ISO-like values interpreted in the source timezone. Ambiguous or nonexistent local times around daylight-saving transitions are rejected rather than guessed.

### `calculate_date_difference`
Returns signed elapsed seconds/days and calendar-day difference between `start` and `end`. Inputs accept ISO dates, local ISO-like datetimes, or RFC 3339 timestamps. Values without an offset are interpreted as UTC.

### `get_date_info`
Returns weekday, ISO week/year, day of year, month length, and leap-year information for a `YYYY-MM-DD` date.

### `parse_datetime`
Parses `value` and returns normalized date/time and Unix timestamps. Optional `format` accepts chrono strftime syntax; optional IANA `timezone` defaults to UTC. Without an explicit format, RFC 3339 and common ISO date/time forms are supported.

### `sleep`
Waits for `seconds` (integer, 0–30). The asynchronous wait is cancelled when its execution future is dropped. The maximum duration is deliberately bounded.

All tools are registered from their TOML definitions in `data/`; agents must explicitly allow the tools in their configuration.
