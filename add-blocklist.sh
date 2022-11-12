#!/bin/bash

# Fill in your instance domain
MASTODON_DOMAIN="gamedev.lgbt"
# Fill in your session id cookie token (found in your browser cookies)
SESSION_ID=""
# Fill in your mastodon session token (found in your browser cookies)
MASTODON_SESSION=""

# Get auth token for creating new block
AUTH_TOKEN=$(
    curl -sL "https://$MASTODON_DOMAIN/admin/domain_blocks/new" \
        -H "Cookie: _session_id=$SESSION_ID; _mastodon_session=$MASTODON_SESSION" \
        | rg -o 'name="authenticity_token"\svalue="(.*?)"' -r '$1'
)

# Read file, skip the first (header line)
cat blocklist.csv | tail -n +2 | while read -r line; do
    DOMAIN=$(echo $line | cut -d ';' -f1)
    REASON=$(echo $line | cut -d ';' -f2)

    curl -sL "https://$MASTODON_DOMAIN/admin/domain_blocks" \
        -X POST \
        -H 'Content-Type: application/x-www-form-urlencoded' \
        -H "Cookie: _session_id=$SESSION_ID; _mastodon_session=$MASTODON_SESSION" \
        --data-raw "authenticity_token=$AUTH_TOKEN&domain_block%5Bdomain%5D=$DOMAIN&domain_block%5Bseverity%5D=suspend&domain_block%5Breject_media%5D=0&domain_block%5Breject_reports%5D=0&domain_block%5Bobfuscate%5D=0&domain_block%5Bprivate_comment%5D=&domain_block%5Bpublic_comment%5D=$REASON&button=" > /dev/null
done


