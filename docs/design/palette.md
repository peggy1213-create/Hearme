# Hearme colour palette: "Quiet Signal"

A calm ink blue that stays out of the way, and one warm signal that means "audio is being captured". Neutrals carry the interface; colour is saved for state and for telling speakers apart. Every text colour below passes WCAG AA (4.5:1) on its background.

## Brand and signal

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--primary` | `#2E44C4` | `#8E9CFF` | Buttons, links, focus rings, the "Me" speaker. White text on it: 7.7:1 |
| `--listening` | `#C2412D` | `#FF8A70` | The listening indicator only. If it's coral, audio is being captured |
| `--highlight` | `#FFE7A3` | `#4A3F17` | Search hits, keywords, bookmarked moments |

## Neutrals

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--canvas` | `#F6F6F3` | `#111318` | App background |
| `--surface` | `#FFFFFF` | `#1A1D24` | Panels, cards |
| `--line` | `#E2E3DF` | `#2C303A` | Borders, dividers |
| `--muted` | `#5B6270` | `#9AA1AE` | Timestamps, hints (5.7:1 light, 6.5:1 dark) |
| `--ink` | `#15181E` | `#ECEDF0` | Transcript text (16.4:1 light, 14.4:1 dark) |

## Status

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--good` | `#1C7A50` | `#5FD39A` | Saved, summary ready |
| `--warning` | `#8F5600` | `#F0B44C` | Low audio, weak connection |
| `--error` | `#B42318` | `#FF8A80` | Capture stopped, permission missing |

## Speakers (assigned in order)

Each speaker always appears with a name and an initial, so colour is never the only cue.

| Speaker | Light | Dark |
| --- | --- | --- |
| Me | `#2E44C4` | `#8E9CFF` |
| Speaker 1 (teal) | `#0F766E` | `#4CCFC2` |
| Speaker 2 (plum) | `#8A3B9A` | `#D49BE6` |
| Speaker 3 (sky) | `#0A6AA6` | `#6EC2F7` |
| Speaker 4 (rose) | `#B0245A` | `#F582A8` |
| Speaker 5 (moss) | `#4A7310` | `#A6D45E` |

Speaker avatars use the speaker colour at 12% opacity (18% in dark mode) as the background, with the initial in the full colour.

## Type

- Chinese: Noto Sans TC
- Latin: Figtree (or another clean sans)
- Timestamps: IBM Plex Mono
