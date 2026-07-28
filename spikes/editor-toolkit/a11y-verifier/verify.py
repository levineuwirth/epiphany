#!/usr/bin/env python3
"""Round 0 accessibility readback verifier.

An AT-SPI client, independent of any candidate's own process, that walks
the live platform accessibility tree (via the AT-SPI2 registry over D-Bus)
looking for an accessible node with a given role and name. This is a real
client query of the tree, per CONTRACT_EDITOR_T4_SPIKE.md Round 0: "Setting
the node in your own process and printing your own struct is NOT a
readback."

Uses gi.repository.Atspi, the official GObject-introspection binding for
AT-SPI2 (the same library backing Orca and Accerciser). This is used in
place of the `atspi` Rust crate as an "equivalent AT-SPI client" (the
contract's own wording) — chosen because its API is stable, documented, and
already verified reachable on this machine, rather than reverse-engineering
an unfamiliar async zbus proxy API under this round's timebox. That
substitution is a named deviation, reported as such.

Usage:
    verify.py --role "push button" --name "EpiphanyProbeButton" [--app-name SUBSTR] [--max-depth N] [--timeout SECONDS]

Exit code 0 and prints "READBACK: PASS" with the path from desktop root to
the matched node, if found within the timeout. Exit code 1 and prints
"READBACK: FAIL" with a dump of what *was* found, if the bus is reachable
but no match appears before the timeout. Exit code 2 and prints
"READBACK: NOT RUN" if the AT-SPI bus itself cannot be reached at all.
"""
import argparse
import sys
import time

try:
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi
except Exception as exc:  # pragma: no cover - environment probe
    print(f"READBACK: NOT RUN — could not import gi.repository.Atspi: {exc}")
    sys.exit(2)


def walk(node, role, name, app_name_substr, max_depth, path, found, all_seen):
    if node is None:
        return
    try:
        node_name = node.get_name()
    except Exception:
        node_name = "<error>"
    try:
        node_role = node.get_role_name()
    except Exception:
        node_role = "<error>"
    all_seen.append(" / ".join(path + [f"{node_role}:{node_name!r}"]))
    if node_role == role and node_name == name:
        found.append(list(path) + [f"{node_role}:{node_name!r}"])
        return
    if max_depth <= 0:
        return
    try:
        n = node.get_child_count()
    except Exception:
        return
    for i in range(n):
        try:
            child = node.get_child_at_index(i)
        except Exception:
            continue
        walk(
            child,
            role,
            name,
            app_name_substr,
            max_depth - 1,
            path + [f"{node_role}:{node_name!r}"],
            found,
            all_seen,
        )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--role", required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--app-name", default=None, help="only descend into apps whose name contains this substring")
    ap.add_argument("--max-depth", type=int, default=12)
    ap.add_argument("--timeout", type=float, default=20.0)
    ap.add_argument("--poll-interval", type=float, default=0.5)
    args = ap.parse_args()

    try:
        Atspi.init()
    except Exception as exc:
        print(f"READBACK: NOT RUN — Atspi.init() failed: {exc}")
        sys.exit(2)

    deadline = time.monotonic() + args.timeout
    last_seen = []
    attempt = 0
    while time.monotonic() < deadline:
        attempt += 1
        try:
            desktop = Atspi.get_desktop(0)
        except Exception as exc:
            print(f"READBACK: NOT RUN — Atspi.get_desktop(0) failed: {exc}")
            sys.exit(2)
        if desktop is None:
            print("READBACK: NOT RUN — Atspi.get_desktop(0) returned None (no AT-SPI registry?)")
            sys.exit(2)

        found = []
        all_seen = []
        try:
            n_apps = desktop.get_child_count()
        except Exception as exc:
            print(f"READBACK: NOT RUN — desktop.get_child_count() failed: {exc}")
            sys.exit(2)

        for i in range(n_apps):
            try:
                app = desktop.get_child_at_index(i)
            except Exception:
                continue
            if app is None:
                continue
            try:
                app_name = app.get_name()
            except Exception:
                app_name = "<error>"
            if args.app_name and args.app_name not in app_name:
                continue
            walk(
                app,
                args.role,
                args.name,
                args.app_name,
                args.max_depth,
                [f"desktop"],
                found,
                all_seen,
            )

        last_seen = all_seen
        if found:
            print("READBACK: PASS")
            print(f"attempt: {attempt}, elapsed: {args.timeout - (deadline - time.monotonic()):.2f}s")
            print("path: " + " / ".join(found[0]))
            print(f"apps enumerated: {n_apps}")
            print("full tree (role:name) seen during the matching walk:")
            for line in all_seen:
                print("  " + line)
            sys.exit(0)

        time.sleep(args.poll_interval)

    print("READBACK: FAIL")
    print(f"no node with role={args.role!r} name={args.name!r} found within {args.timeout}s ({attempt} attempts)")
    print("nodes actually seen (role:name), last attempt:")
    if not last_seen:
        print("  <none — desktop had 0 matching/enumerable apps>")
    for line in last_seen:
        print("  " + line)
    sys.exit(1)


if __name__ == "__main__":
    main()
