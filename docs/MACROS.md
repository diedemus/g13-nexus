# Macro recorder

Macros are stored per profile and per M bank.

## Recording

1. Select M1, M2, or M3.
2. Press MR.
3. Select the programmable G13 control that should own the macro.
4. Type the macro sequence.
5. Press MR again to finish and save.

While recording, the floating Macro Recording window displays the active bank, destination control, accepted key-down/key-up events, and inter-event timing. The destination control is visibly outlined while it is being recorded. Controls with a stored macro show an `M` badge in the main hardware view.

## Playback

Macro playback is asynchronous so input processing is not blocked by recorded delays. Recorded modifier/key release handling prevents a completed macro from leaving modifiers logically held.

## Capture sources

The daemon can record Linux keyboard events where session ACLs permit. The focused GUI also provides a Wayland-safe fallback, with duplicate edges de-duplicated when both paths see the same event.
