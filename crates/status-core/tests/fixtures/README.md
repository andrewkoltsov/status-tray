Fixtures for `status-core` parser and diff tests.

Synthetic (hand-written, Atlassian Statuspage `/api/v2/summary.json` schema):
`all_green.json`, `incident_open.json`, `incident_updated.json`, `maintenance.json`,
`unknown_enums.json`. Replace `all_green.json` with a real capture when convenient:

    curl https://www.githubstatus.com/api/v2/summary.json | jq . > all_green.json

("Resolved" is `all_green.json` seen after `incident_open.json`.)

Real captures (taken from GitHub Actions; the status hosts are not reachable from the dev
sandbox). Long texts/maps were trimmed, structure is untouched:
- `aws_currentevents.json`: https://health.aws.amazon.com/public/currentevents, re-encoded as
  UTF-8 (on the wire it is UTF-16); each `event_log` cut to its last 3 entries,
  `impacted_services` cut to 5 entries.
- `gcp_none_open.json`: one real, closed incident from https://status.cloud.google.com/incidents.json
  (texts cut to 500 chars, `affected_products` cut to 3).
- `azure_empty.xml`: the real Azure feed when no widespread incident is active.

Derived / synthetic:
- `gcp_open.json`: the real incident above with `end` removed and a European location added
  (open), the same incident closed, plus a synthetic open incident in `asia-east1`.
- `aws_currentevents_utf16.bin`: `aws_currentevents.json` as UTF-16LE with BOM; regenerate with
  `python3 scripts/gen_aws_utf16_fixture.py`.
- `azure_active.xml`: synthetic feed with 2 realistic items (HTML-escaped descriptions).
