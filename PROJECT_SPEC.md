# Project spec — Kavach

## What it is

A privacy-first, local-first application builder. A user describes an application in one
sentence; Kavach generates a working project, verifies it against its own tests, and reports
what it did — with everything running on the user's machine.

## Who it is for

Someone who wants a small application built and does not want to send their prompt, their data
or their code to a hosted service.

## Hard requirements

1. **Nothing leaves the machine.** No telemetry, no hosted API, no metered service. The only
   socket the host opens is loopback, to a model server the user runs.
2. **Free in every sense.** Apache-2.0, no account, no licence key, builds and runs offline.
3. **No claim without evidence.** A task is verified only against a command that ran.
4. **Windows, Linux and macOS**, with the desktop shell as the primary surface.

## What it generates

A Node 20 project: HTTP server, validated store, data pipeline (10 functions), analysis
library (28 functions), fundamentals engine (39 ratios), and a test suite that ships with the
app and passes under Node's permission model.

## What it is not

Not a stock product, not a trading terminal, not a data vendor. `examples/stocklab/` is a
worked example of generated output, not the product.

## Definition of done

`scripts/verify-all.sh` green, `scripts/verify-zip.sh` green on a fresh extraction, and the
known-issues list truthful. See `CHECKLIST.md`.
