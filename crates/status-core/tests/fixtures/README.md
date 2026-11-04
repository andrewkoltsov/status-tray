These fixtures are SYNTHETIC. They follow the Atlassian Statuspage `/api/v2/summary.json`
schema but were written by hand (the environment that generated the skeleton could not reach
any status page). Replace `all_green.json` with a real capture when convenient:

    curl https://www.githubstatus.com/api/v2/summary.json | jq . > all_green.json

(then trim the component list). "Resolved" is `all_green.json` seen after `incident_open.json`.
