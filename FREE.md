# Kavach is free — in every sense that matters

This document states what "free" means here, how it is verified, and the one place where a
cost genuinely exists if you want it.

---

## 1. Free to use, modify and redistribute

Apache-2.0. No account, no licence key, no activation server, no subscription, no seat count.
The full text is in `LICENSE`.

## 2. Free to run

- **No backend.** There is no Kavach server to host, so there is no hosting bill.
- **No account.** Nothing to sign up for, nothing to authenticate against.
- **No metered API.** The application never calls a paid service. The only socket the host
  opens is loopback, to a model server you run yourself.
- **No telemetry.** Not "opt-out" — absent. The policy engine denies the category outright.
- **No bundled model weights.** Nothing is downloaded or shipped, so there is no per-token or
  per-seat model cost. If you run a model, it is one you already have.

Verified by `scripts/check-free.py`, which scans every shipped source file for metered
service hosts, telemetry SDKs and commercial machinery, asserts the host crate still has zero
dependencies, and checks the product licence. It runs in `scripts/verify-all.sh`.

## 3. Free to build

Three routes, all zero-cost. Pick one.

### Local — no CI at all

```bash
./scripts/build-local.sh --test        # Linux, macOS
```
```powershell
.\scripts\build-windows.ps1 -Test      # Windows
```

Everything runs on your machine. The only network use is `crates.io` fetching the crate graph
once, which is free bandwidth.

### Self-hosted runner — free CI, private repository

GitHub Actions is **free for public repositories and for self-hosted runners**, and metered
only for private repositories on hosted runners [cite:418b9029]. So if the repository stays
private and you own a Windows machine, register it as a self-hosted runner and dispatch
`.github/workflows/build-self-hosted.yml`. Nothing is billed.

Setup is in the header of that workflow: Settings → Actions → Runners → New self-hosted runner.

### Hosted runner — free if the repository is public

The existing `build.yml` runs on `windows-latest`. On a public repository those minutes are
free and unlimited [cite:418b9029]. On a private one they come from the 2,000-minute monthly
allowance (Windows billed at 2×), which is what ran out in October.

**This is a decision only you can make:** making the repository public makes CI free and
exposes the source. The code is Apache-2.0 and intended to be open, so that may be exactly
what you want — but it is not a decision to take on your behalf.

## 4. Free to sign — with an honest caveat

A certificate from a public CA costs money: a few hundred dollars a year for an OV
certificate, more for EV plus a hardware token. That is the **one genuine cost** in the whole
project, and it is a business cost, not a technical one.

What is free: `scripts/make-self-signed-cert.ps1` creates a self-signed code-signing
certificate, exports it, trusts it on the local machine, signs both installers and verifies
the signatures. It costs nothing and it does what a signature mechanically does — proves the
binary was not altered after signing.

What it does not do: satisfy SmartScreen for other people. Windows only builds reputation for
publicly-trusted CAs, so recipients still see "Windows protected your PC" unless they install
your certificate as a trusted publisher. Anyone who tells you otherwise is selling something.

So: free for internal distribution, paid for public distribution. `WINDOWS-BUILD.md` §7 has
the configuration for the paid route when you want it.

## 5. Free to extend

No plugin marketplace, no paid tiers, no feature gates. The 20 commands in the host crate are
the whole surface, and the source is the documentation.

---

## What "free" does not mean

- **Free of the cost of a Windows machine** to run the smoke test.
- **Free of a certificate** if you want to distribute publicly without warnings.
- **Free of effort.** The remaining work in `PRODUCTION-READINESS.md` is real work, just not
  paid work.

## Verifying this document

```bash
python3 scripts/check-free.py --verbose     # the audit
./scripts/verify-all.sh                     # everything, including the audit
```
