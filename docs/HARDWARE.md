# Logitech G13 hardware map

USB identity: `046d:c21c`.

## Main keys

G1-G22 are exposed by the current Linux G13 driver as `KEY_MACRO1` through `KEY_MACRO22`.

## Mode controls

- M1: `KEY_MACRO_PRESET1`
- M2: `KEY_MACRO_PRESET2`
- M3: `KEY_MACRO_PRESET3`
- MR: `KEY_MACRO_RECORD_START`

## LCD/menu area

The four rectangular LCD buttons use `KEY_KBD_LCD_MENU1` through `KEY_KBD_LCD_MENU4`. When unbound in Nexus they retain the built-in page behavior:

- LCD1: previous page
- LCD2: next page
- LCD3: display on/off
- LCD4: Status/Home

The round button on the left is `KEY_KBD_LCD_MENU5` and is programmable.

The dedicated round lighting button is `KEY_LIGHTS_TOGGLE`. Nexus intentionally leaves it as a fixed G13 lighting toggle and does not expose it as a programmable GUI control.

## Thumb cluster

- left thumb button: `BTN_BASE`
- bottom thumb button: `BTN_BASE2`
- stick click: `BTN_THUMB`
- left: `ABS_X-`
- right: `ABS_X+`
- up: `ABS_Y-`
- down: `ABS_Y+`

The joystick's physical rest point is not assumed to be 127/127. Nexus reads the current ABS_X/ABS_Y values at connect time and allows per-profile manual center overrides.
