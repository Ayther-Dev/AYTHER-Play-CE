# Game controls and Runtime beta.2

Play targets Runtime `v0.1.0-beta.2`. Its definitive public distribution was not
available at validation time (2026-09-05); do not infer binary compatibility from
source-contract tests. The exact source revision is in
[`tools/runtime-lock.json`](../tools/runtime-lock.json). Runtime's canonical source
documentation is `docs/input-map.md` in that checkout.

Play writes a complete TOML map from Settings > Game controls and passes its path
as one process argument after `--input-map`, including when it contains spaces.
With no overrides it omits the argument. Runtime then uses these defaults:

```toml
[keyboard]
up = "Up"
down = "Down"
left = "Left"
right = "Right"
b = "Z"
a = "X"
y = "A"
x = "S"
l = "Q"
r = "W"
start = "Return"
select = "Right Shift"

[gamepad]
b = "south"
a = "east"
y = "west"
x = "north"
l = "leftshoulder"
r = "rightshoulder"
start = "start"
select = "back"
```

Keyboard values are SDL scancode names (`SDL_GetScancodeFromName`). The accepted
gamepad values are exactly the eight physical button names above. Gamepad
directions always combine the D-pad and left stick (threshold 16000); they cannot
be reassigned in the map. Runtime combines the keyboard and controller inputs.

Runtime permits partial maps: omitted actions retain their defaults. Play swaps
conflicting assignments and emits a complete map. Effective duplicate bindings,
including collisions with inherited defaults, are rejected by Runtime. Unknown
sections/actions, non-string values, unknown scancodes/buttons, gamepad direction
bindings, malformed TOML and unreadable files also fail the complete map.

Failure occurs before SDL/Vulkan startup: Runtime emits an `AYTHER_STATUS` warning
with reason `input.map_invalid`, describes `--input-map` on stderr and exits 78.
Play retains that failure reason, reports an unsuccessful session and directs the
user to Settings > Game controls. The real-process regression is explicitly
ignored during ordinary development and required by release integration.

Without any map, Runtime also accepts its legacy Backspace alias for Select.
Supplying a valid map, even a partial one, disables that alias. Source tests and
TOML tests do not verify physical keyboard/controller behavior inside a game;
that remains a separate manual release check.
