# ADR-2610091500: Splunk setup finds the API

**Status:** Accepted
**Date:** 2026-10-09
**Drivers:** Setting up Splunk took the user four tries, each ending in an error they had to decode:
the certificate was Splunk's own ("UnknownIssuer"), the address was Splunk Web (a 404 page), and a
typed port was wrong (8090 for 8089). The user: "cant we help the user there". People copy the
address from their browser, which is Splunk Web on port 443; Wardian needs Splunk's management API,
usually on port 8089, which most Splunk servers protect with Splunk's built-in certificate.

## Decision

1. **The address is read generously.** Settings → Splunk accepts a host name, or any URL on the
   server, such as one pasted from the browser bar (`https://host/en-US/app/search/...`). Wardian
   keeps the scheme (https when none), the host and the port, and drops the path.
2. **Wardian finds the API.** "Test and save" tries the address as given. When that is not Splunk's
   API (a page that is not the REST answer, a 404 from Splunk Web, a refused or broken connection),
   it tries the same host on port 8089. The first that answers as Splunk's API is saved, and the
   result says which: "Splunk's API is on port 8089; saved https://host:8089." Any other port the
   user typed is tried first, so a non-standard API port still works. No other hosts or ports are
   tried.
3. **Splunk's own certificate is named, and trusted only when the user says so.** When the TLS
   failure is a certificate Wardian cannot trust, the message says whether it is Splunk's built-in
   certificate (`SplunkServerDefaultCert`, issued by `SplunkCommonCA`) or another one, and offers one
   button: "Trust Splunk's own certificate and try again" (it ticks "Allow a self-signed
   certificate" and saves again), or, for another issuer, the CA file field. Nothing is trusted
   without that click.
4. **The form says what it wants:** the address field's hint reads "Your Splunk's address, as in
   your browser"; a short line says Wardian finds the API port itself.

## Consequences

- Pasting the browser's address and ticking one button is enough for a standard Splunk.
- Setup may make up to two extra short requests to the same host.
- A Splunk whose API is only on a port other than the one given or 8089 still needs that port typed.

## Implementation

- `domain/splunk.rs`: reading the address, and the candidate list, as pure functions (tests named
  `splunk_setup_…`). `adapters/secondary/splunk_rest.rs`: classify a failure (not the API, wrong
  certificate and whether it is Splunk's default, unreachable) and report the certificate's issuer.
  `usecases/splunk.rs`: try the candidates in order, save the first that works.
- `static/index.html`: the hint, the result line, the "Trust Splunk's own certificate" button.
- `tests/run-splunk-e2e.sh`: the fake Splunk also serves a "web" port that answers 404 pages, and a
  TLS port with a self-signed certificate; the test saves with the web address and gets the API
  port, and sees and uses the trust button.
- docs/site/splunk.md, troubleshooting, CHANGELOG.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release splunk_setup_`

## References

- ADR-2610081500 (keys and settings in one place)
