# v0.4.2 — direct rig controls and simultaneous live outputs

Every rig tile and Rig Manager row now has a one-click **Reset tracking** and
**Settings** button. Clicking Settings again hides its inspector. Inspectors open
away from their launching controls so those buttons remain reachable.

Click a rig image once to expand it. Double-click the expanded image to return
to all rigs, or use its grid button. Calibration editing keeps its point/drag
controls. The toolbar uses an icon without the old Rig grid text, a distinct
grid/settings symbol, a bug-shaped debug icon, and hover titles/descriptions.

Settings → Outputs now offers **ZeroMQ + UDP**, **ZeroMQ**, and **UDP socket**.
The bundled fresh rig defaults enable both outputs. Switching to a single
transport remembers custom ports so selecting both again restores them.
There is no disabled choice in the selector. Turning off the last ZeroMQ
publisher enables UDP, keeping one selected output available.

Existing configuration text remains authoritative, including intentionally
disabled transports in recorded-video/headless workflows. Merely opening the
settings does not rewrite a legacy configuration. Neither tracking algorithms
nor numeric acceptance limits change in this patch.

The local 17° A01–A04 device setup retains the previously stored lab rotations,
as confirmed by the user. UDP 1111–1114 continues feeding the existing bridges
on 9871–9874; simultaneous direct ZeroMQ uses free ports 9881–9884. This keeps
the running bridges available and avoids two publishers sharing a bind port.
These are device defaults and a portable backup, not bundled rig calibrations.

The v0.4.1 source tag and published packages remain unchanged.
