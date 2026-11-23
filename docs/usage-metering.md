# Model usage metering

EME records model usage per provider invocation. A retry or fallback is a separate invocation, so a failed candidate is not hidden by the succeeding candidate.

## Accounting data

Each record includes:

- Run, iteration, agent, and authenticated principal identifiers.
- Provider-qualified model identity and invocation outcome.
- Input, output, cached-input, and reasoning token dimensions.
- A provenance for each dimension: provider-reported, platform-estimated, or unavailable.
- An optional provider-reported monetary cost, kept separate from calculated cost.
- The invocation timestamp used to select a pricing version.

Usage records are append-only and keyed by invocation ID. Replaying the same record is idempotent; reusing the ID with different content is rejected. Records are stored separately from run events and OpenTelemetry, so telemetry retention does not determine accounting retention.

## Pricing

Pricing versions are immutable and scoped to a provider/model pair. Effective periods are half-open: `effective_from <= invocation_time < effective_until`; an omitted end date is open-ended. Overlapping periods for the same provider/model are rejected.

Cached input tokens are a subset of input tokens, and reasoning tokens are a subset of output tokens. Cost calculation subtracts these subsets from the standard input/output dimensions before applying their dedicated rates, avoiding double-counting.\n\nEach entry in `rates_per_million` is an integer number of currency micro-units per one million units of the named dimension. For example, a USD rate of $2.00 per million input tokens is stored as `2000000`. Calculations use integer arithmetic; they do not use floating point.

The dashboard's Pricing catalogue lets an authenticated user add a new version. Rates are entered in currency units per million tokens and converted to micro-units before persistence. Existing versions cannot be edited or deleted; add a new version with a new effective period instead.

## Cost semantics

- Provider-reported costs take precedence and are marked `provider_reported`. The OpenAI-compatible adapter retains a reported cost only when the response also supplies an explicit currency; it does not guess the currency.
- Otherwise, the matching effective pricing version is used to calculate cost.
- If any reported usage dimension has no rate, or a dimension required by the pricing entry is unavailable, the calculated result is marked partial.
- If no billable dimension can be calculated, cost is unavailable.
- Estimated token quantities remain marked as estimated in the cost result.
- No matching price is represented as unpriced; it is never displayed as zero.

## API

- `GET /runs/usage` returns the authenticated user's invocation records with cost estimates.
- `GET /runs/{run_id}/usage` returns invocation details and a run-level aggregate across all turns, retries, and fallbacks.
- `GET /runs/pricing` returns registered immutable pricing versions.
- `POST /runs/pricing` registers a new pricing version. It requires an authenticated session and rejects invalid, duplicate, or overlapping versions.

The Usage dashboard consumes these endpoints and provides date filtering, token totals, provider/model breakdowns, cost provenance, and invocation history. Invocation rows link to a run-specific drill-down with aggregate totals and the underlying attempts.
