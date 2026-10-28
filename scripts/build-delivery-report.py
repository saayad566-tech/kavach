#!/usr/bin/env python3
"""Build the Kavach delivery report.

The report is generated from this script rather than written by hand, so it can be regenerated
whenever the facts change. Run:

    python3 scripts/build-delivery-report.py [out.pdf]
    python3 scripts/run_qa.py <out.pdf>       # the PDF skill's layout gate
"""

import sys

from reportlab.lib import colors
from reportlab.lib.enums import TA_LEFT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import mm
from reportlab.platypus import (
    ListFlowable,
    ListItem,
    PageBreak,
    Paragraph,
    SimpleDocTemplate,
    Table,
    TableStyle,
)

OUT = sys.argv[1] if len(sys.argv) > 1 else "Kavach-Delivery-Report.pdf"

PAGE_W, PAGE_H = A4
doc = SimpleDocTemplate(
    OUT, pagesize=A4,
    leftMargin=20 * mm, rightMargin=20 * mm, topMargin=22 * mm, bottomMargin=20 * mm,
    title="Kavach — Delivery Report", author="Kavach build",
)
CW = doc.width

styles = getSampleStyleSheet()
body = ParagraphStyle("body", parent=styles["Normal"], fontName="Helvetica",
                      fontSize=9.5, leading=13, spaceAfter=7, alignment=TA_LEFT)
h1 = ParagraphStyle("h1", parent=styles["Heading1"], fontName="Helvetica-Bold",
                    fontSize=19, leading=23, spaceAfter=4, textColor=colors.HexColor("#12212f"))
sub = ParagraphStyle("sub", parent=body, fontSize=10.5, leading=14,
                     textColor=colors.HexColor("#4a5b6b"), spaceAfter=14)
h2 = ParagraphStyle("h2", parent=styles["Heading2"], fontName="Helvetica-Bold",
                    fontSize=12.5, leading=16, spaceBefore=12, spaceAfter=5,
                    textColor=colors.HexColor("#12212f"))
cell = ParagraphStyle("cell", parent=body, fontSize=8.5, leading=11, spaceAfter=0)
cell_b = ParagraphStyle("cellb", parent=cell, fontName="Helvetica-Bold")


def header_footer(canvas, doc_):
    canvas.saveState()
    canvas.setFont("Helvetica", 8)
    canvas.setFillColor(colors.HexColor("#6b7c8c"))
    canvas.drawString(doc_.leftMargin, PAGE_H - 13 * mm, "Kavach — Delivery Report")
    canvas.drawRightString(PAGE_W - doc_.rightMargin, PAGE_H - 13 * mm, "28 October 2026")
    canvas.setStrokeColor(colors.HexColor("#d7dfe8"))
    canvas.line(doc_.leftMargin, PAGE_H - 15 * mm, PAGE_W - doc_.rightMargin, PAGE_H - 15 * mm)
    canvas.drawRightString(PAGE_W - doc_.rightMargin, 12 * mm, "Page %d" % canvas.getPageNumber())
    canvas.restoreState()


def table(rows, widths):
    data = []
    for index, row in enumerate(rows):
        style = cell_b if index == 0 else cell
        data.append([Paragraph(str(value), style) for value in row])
    t = Table(data, colWidths=widths, repeatRows=1, hAlign="LEFT")
    t.setStyle(TableStyle([
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("TOPPADDING", (0, 0), (-1, -1), 4),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 4),
        ("LEFTPADDING", (0, 0), (-1, -1), 5),
        ("RIGHTPADDING", (0, 0), (-1, -1), 5),
        ("LINEBELOW", (0, 0), (-1, -1), 0.4, colors.HexColor("#dfe6ee")),
        ("BACKGROUND", (0, 0), (-1, 0), colors.HexColor("#eef3f8")),
        ("LINEBELOW", (0, 0), (-1, 0), 0.8, colors.HexColor("#b9c6d4")),
    ]))
    return t


story = [
    Paragraph("Kavach — Delivery Report", h1),
    Paragraph("A privacy-first, local-first application builder. This report states what is built, "
              "what is proven, what is not, and what it cost.", sub),
    Paragraph(
        "Kavach turns a sentence describing an application into a working project: a Node HTTP "
        "server, a validated store, a data pipeline with 38 analysis functions, and a test suite "
        "that ships with the app. The desktop shell is a Tauri 2 application whose webview holds "
        "no filesystem, shell or network permission; every privileged operation crosses into "
        "Rust through one of 21 audited commands. Nothing leaves the machine, nothing is "
        "metered, and nothing requires an account.", body),

    Paragraph("Status against the readiness criteria", h2),
    table([
        ["Area", "State", "Note"],
        ["Host logic", "Yes", "101 Rust tests, zero dependencies, builds offline"],
        ["Generated applications", "Yes", "Pass their own suite: 50 tests per app"],
        ["Data pipeline", "Yes", "10 functions, CSV and JSON, domain-neutral"],
        ["Analysis", "Yes", "28 functions, each with a test"],
        ["Sandbox for generated code", "Yes", "Filesystem confined; child processes and network denied"],
        ["Windows installers", "Not at this commit", "Last proven build predates these changes"],
        ["Windows smoke test", "No", "Written, never run on Windows"],
        ["Code signing", "Partly", "Free self-signed route; public release needs a paid certificate"],
        ["Updater", "No, deliberately", "An auto-updater phones home; manual upgrade is the path"],
    ], [46 * mm, 30 * mm, CW - 76 * mm]),

    Paragraph("Evidence", h2),
    Paragraph("Every number below comes from a command in the repository that can be re-run. "
              "None of it is an estimate.", body),
    table([
        ["Check", "Result"],
        ["cargo test --offline", "101 passed, 0 failed"],
        ["Generated-app pipeline and analysis tests", "42 passed"],
        ["Desktop UI render and accessibility", "27 of 27 checks"],
        ["Offline checks (scripts/verify-all.sh)", "9 of 9 passed"],
        ["Dependency graph", "405 crates resolve; tauri 2.12.5"],
    ], [CW - 45 * mm, 45 * mm]),

    PageBreak(),

    Paragraph("How generated code is contained", h2),
    Paragraph(
        "Generated tests run under Node's permission model, which confines the filesystem to the "
        "project and the OS temp directory and denies child processes. Node 20 has no switch for "
        "the network, so a preload denies net, tls, dns, http, https, dgram and the global fetch "
        "before the test file loads.", body),
    Paragraph(
        "Five tests prove the containment rather than asserting it: a read of /etc/hostname "
        "fails, spawning a shell fails, net.connect fails, fetch fails, and an ordinary test that "
        "writes its own scratch files still passes.", body),
    Paragraph(
        "One finding is worth recording. The obvious implementation — node --test with "
        "--allow-child-process, because the runner spawns a child per file — is not a sandbox. "
        "Node's own warning says that flag could invalidate the model, and it does: with it, a "
        "test spawned cat /etc/hostname and read the result. Running each test file directly "
        "keeps everything in one process, which is why the grant stays meaningful.", body),
    Paragraph(
        "The preload is a preload, not a kernel boundary. A determined test could reload a module "
        "and get past it. It stops ordinary code, which is what a generated test is.", body),

    Paragraph("What changed recently", h2),
    ListFlowable([
        ListItem(Paragraph("<b>Task ledger and memory.</b> Section 8 of the brief: eight task "
                           "states with acceptance criteria and evidence, notes for changes and failures, "
                           "and a store that refuses a verified state with no evidence.", body)),
        ListItem(Paragraph("<b>Network denial.</b> The last gap in the sandbox: Node 20 cannot "
                           "restrict the network, so a preload does it, with two tests proving it.", body)),
        ListItem(Paragraph("<b>Sandbox cannot degrade silently.</b> The builder reads "
                           "node --version and refuses to verify on anything older than 20; generated "
                           "package.json declares engines.node >= 20.", body)),
        ListItem(Paragraph("<b>Declared toolchain.</b> rust-toolchain.toml pins the channel, "
                           "rust-version states the minimum, and scripts/check-toolchain.py names the "
                           "install line for anything missing.", body)),
        ListItem(Paragraph("<b>Every command reachable.</b> Four orphaned commands now drive a "
                           "model panel; three are declared internal with reasons, enforced by a checker.", body)),
        ListItem(Paragraph("<b>Domain-neutral output.</b> The analysis panel discovers its fields "
                           "from the inferred schema instead of naming volume and sector.", body)),
        ListItem(Paragraph("<b>Resource limits and a durable store.</b> Kernel-enforced CPU and "
                           "write caps; versioned index with automatic recovery from a corrupt primary.", body)),
    ], bulletType="bullet", leftIndent=12),

    Paragraph("Corrections made during the work", h2),
    Paragraph("Three claims in earlier reports were wrong. They are recorded here because a report "
              "that quietly drops its mistakes is worth less than one that names them.", body),
    table([
        ["Claim", "What was actually true"],
        ["crates.io is blocked",
         "Only the website returns 403. The sparse index and crate files both download; 405 crates "
         "resolve. The original test used HEAD requests, which the CDN rejects."],
        ["Sandboxing needs Wasmtime",
         "Generated applications are JavaScript; Wasmtime runs WebAssembly. The right mechanism, "
         "Node's permission model, was already installed."],
        ["The generated app is generic",
         "It hardcoded volume and sector and shipped a stock dataset, so every non-stock app asked "
         "for fields it did not have."],
    ], [48 * mm, CW - 48 * mm]),

    Paragraph("What is not done", h2),
    Paragraph(
        "No installer matches the current commit. Building the Tauri shell needs webkit2gtk on "
        "Linux (which needs root) or a Windows toolchain, and neither exists in the environment "
        "this was built in. The Windows smoke test has never run: nineteen checks are waiting on a "
        "machine that is not available here.", body),
    Paragraph(
        "The shortest path to closing both is the free self-hosted runner already in the "
        "repository: one run on a Windows machine produces an installer, the smoke-test evidence, "
        "and a green build against this exact commit.", body),

    Paragraph("Cost", h2),
    Paragraph(
        "Zero. No backend, no account, no metered API, no telemetry, no bundled model weights. "
        "Building is free locally or on a self-hosted runner. The one genuine cost is a public "
        "code-signing certificate, which is a business decision rather than a technical one; a "
        "free self-signed route exists for internal distribution.", body),
]

doc.build(story, onFirstPage=header_footer, onLaterPages=header_footer)
print("wrote", OUT)
