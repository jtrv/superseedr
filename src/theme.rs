// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeName {
    CatppuccinMocha,
    Neon,
    #[serde(alias = "candly_land_pink")]
    CandyLandPink,
    Dracula,
    Nord,
    GruvboxDark,
    TokyoNight,
    OneDark,
    SolarizedDark,
    Monokai,
    EverforestDark,
    RosePine,
}

impl Default for ThemeName {
    fn default() -> Self {
        Self::CatppuccinMocha
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ThemeEffects {
    pub glow_enabled: bool,
    pub flicker_hz: f32,
    pub flicker_intensity: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeSemantic {
    pub text: Color,
    pub subtext0: Color,
    pub subtext1: Color,
    pub overlay0: Color,
    pub surface0: Color,
    pub surface1: Color,
    pub surface2: Color,
    pub border: Color,
    pub white: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeHeatmap {
    pub low: Color,
    pub medium: Color,
    pub high: Color,
    pub empty: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeStream {
    pub inflow: Color,
    pub outflow: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeDust {
    pub foreground: Color,
    pub midground: Color,
    pub background: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeCategorical {
    pub rosewater: Color,
    pub flamingo: Color,
    pub pink: Color,
    pub mauve: Color,
    pub red: Color,
    pub maroon: Color,
    pub peach: Color,
    pub yellow: Color,
    pub green: Color,
    pub teal: Color,
    pub sky: Color,
    pub sapphire: Color,
    pub blue: Color,
    pub lavender: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeScale {
    pub speed: [Color; 8],
    pub ip_hash: [Color; 14],
    pub heatmap: ThemeHeatmap,
    pub stream: ThemeStream,
    pub dust: ThemeDust,
    pub categorical: ThemeCategorical,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: ThemeName,
    pub effects: ThemeEffects,
    pub semantic: ThemeSemantic,
    pub scale: ThemeScale,
}

impl Theme {
    pub fn builtin(name: ThemeName) -> Self {
        match name {
            ThemeName::CatppuccinMocha => Self::catppuccin_mocha(),
            ThemeName::Neon => Self::neon(),
            ThemeName::CandyLandPink => Self::candy_land_pink(),
            ThemeName::Dracula => Self::dracula(),
            ThemeName::Nord => Self::nord(),
            ThemeName::GruvboxDark => Self::gruvbox_dark(),
            ThemeName::TokyoNight => Self::tokyo_night(),
            ThemeName::OneDark => Self::one_dark(),
            ThemeName::SolarizedDark => Self::solarized_dark(),
            ThemeName::Monokai => Self::monokai(),
            ThemeName::EverforestDark => Self::everforest_dark(),
            ThemeName::RosePine => Self::rose_pine(),
        }
    }

    pub fn catppuccin_mocha() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(245, 224, 220),
            flamingo: Color::Rgb(242, 205, 205),
            pink: Color::Rgb(245, 194, 231),
            mauve: Color::Rgb(203, 166, 247),
            red: Color::Rgb(243, 139, 168),
            maroon: Color::Rgb(235, 160, 172),
            peach: Color::Rgb(250, 179, 135),
            yellow: Color::Rgb(249, 226, 175),
            green: Color::Rgb(166, 227, 161),
            teal: Color::Rgb(148, 226, 213),
            sky: Color::Rgb(137, 220, 235),
            sapphire: Color::Rgb(116, 199, 236),
            blue: Color::Rgb(137, 180, 250),
            lavender: Color::Rgb(180, 190, 254),
        };

        Self {
            name: ThemeName::CatppuccinMocha,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(205, 214, 244),
                subtext1: Color::Rgb(186, 194, 222),
                subtext0: Color::Rgb(166, 173, 200),
                overlay0: Color::Rgb(108, 112, 134),
                surface2: Color::Rgb(88, 91, 112),
                surface1: Color::Rgb(69, 71, 90),
                surface0: Color::Rgb(49, 50, 68),
                border: Color::Rgb(88, 91, 112),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    categorical.sky,
                    categorical.green,
                    categorical.yellow,
                    categorical.peach,
                    categorical.maroon,
                    categorical.red,
                    categorical.flamingo,
                    categorical.pink,
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.mauve,
                    medium: categorical.mauve,
                    high: categorical.mauve,
                    empty: Color::Rgb(69, 71, 90),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.green,
                    midground: categorical.blue,
                    background: Color::Rgb(88, 91, 112),
                },
                categorical,
            },
        }
    }

    pub fn neon() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(255, 220, 245),
            flamingo: Color::Rgb(255, 150, 230),
            pink: Color::Rgb(255, 70, 230),
            mauve: Color::Rgb(210, 90, 255),
            red: Color::Rgb(255, 60, 120),
            maroon: Color::Rgb(255, 90, 160),
            peach: Color::Rgb(255, 170, 80),
            yellow: Color::Rgb(255, 240, 90),
            green: Color::Rgb(100, 255, 190),
            teal: Color::Rgb(0, 255, 255),
            sky: Color::Rgb(80, 220, 255),
            sapphire: Color::Rgb(40, 190, 255),
            blue: Color::Rgb(40, 110, 255),
            lavender: Color::Rgb(190, 170, 255),
        };

        Self {
            name: ThemeName::Neon,
            effects: ThemeEffects {
                glow_enabled: true,
                flicker_hz: 18.0,
                flicker_intensity: 0.35,
            },
            semantic: ThemeSemantic {
                text: Color::Rgb(230, 255, 255),
                subtext1: Color::Rgb(140, 230, 245),
                subtext0: Color::Rgb(90, 200, 220),
                overlay0: Color::Rgb(30, 70, 95),
                surface2: Color::Rgb(18, 40, 64),
                surface1: Color::Rgb(12, 30, 52),
                surface0: Color::Rgb(8, 22, 42),
                border: Color::Rgb(18, 40, 64),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    Color::Rgb(200, 255, 255),
                    Color::Rgb(120, 255, 240),
                    Color::Rgb(60, 245, 255),
                    Color::Rgb(80, 190, 255),
                    Color::Rgb(170, 120, 255),
                    Color::Rgb(255, 90, 230),
                    Color::Rgb(255, 60, 190),
                    Color::Rgb(255, 40, 150),
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.mauve,
                    medium: categorical.pink,
                    high: categorical.teal,
                    empty: Color::Rgb(30, 45, 65),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.green,
                    midground: categorical.blue,
                    background: Color::Rgb(40, 60, 80),
                },
                categorical,
            },
        }
    }

    pub fn candy_land_pink() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(255, 232, 244),
            flamingo: Color::Rgb(255, 210, 236),
            pink: Color::Rgb(255, 176, 220),
            mauve: Color::Rgb(226, 180, 255),
            red: Color::Rgb(255, 176, 214),
            maroon: Color::Rgb(255, 192, 224),
            peach: Color::Rgb(255, 202, 178),
            yellow: Color::Rgb(255, 234, 190),
            green: Color::Rgb(210, 236, 216),
            teal: Color::Rgb(196, 230, 226),
            sky: Color::Rgb(190, 216, 255),
            sapphire: Color::Rgb(170, 200, 255),
            blue: Color::Rgb(150, 184, 255),
            lavender: Color::Rgb(214, 190, 255),
        };

        Self {
            name: ThemeName::CandyLandPink,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(255, 250, 254),
                subtext1: Color::Rgb(252, 228, 242),
                subtext0: Color::Rgb(242, 204, 230),
                overlay0: Color::Rgb(224, 182, 210),
                surface2: Color::Rgb(186, 128, 164),
                surface1: Color::Rgb(132, 68, 112),
                surface0: Color::Rgb(112, 52, 96),
                border: Color::Rgb(204, 144, 184),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    Color::Rgb(255, 246, 252),
                    Color::Rgb(255, 224, 244),
                    Color::Rgb(255, 202, 236),
                    Color::Rgb(255, 180, 228),
                    Color::Rgb(255, 158, 220),
                    Color::Rgb(255, 136, 212),
                    Color::Rgb(255, 114, 204),
                    Color::Rgb(255, 92, 196),
                ],
                ip_hash: [
                    categorical.rosewater,
                    categorical.flamingo,
                    categorical.pink,
                    categorical.mauve,
                    categorical.red,
                    categorical.maroon,
                    categorical.peach,
                    categorical.yellow,
                    categorical.lavender,
                    categorical.sky,
                    categorical.sapphire,
                    categorical.blue,
                    categorical.teal,
                    categorical.green,
                ],
                heatmap: ThemeHeatmap {
                    low: categorical.rosewater,
                    medium: categorical.pink,
                    high: categorical.mauve,
                    empty: Color::Rgb(132, 68, 112),
                },
                stream: ThemeStream {
                    inflow: categorical.sky,
                    outflow: categorical.pink,
                },
                dust: ThemeDust {
                    foreground: categorical.pink,
                    midground: categorical.lavender,
                    background: Color::Rgb(156, 86, 130),
                },
                categorical,
            },
        }
    }

    pub fn dracula() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(248, 248, 242),
            flamingo: Color::Rgb(255, 184, 108),
            pink: Color::Rgb(255, 121, 198),
            mauve: Color::Rgb(189, 147, 249),
            red: Color::Rgb(255, 85, 85),
            maroon: Color::Rgb(255, 110, 139),
            peach: Color::Rgb(255, 184, 108),
            yellow: Color::Rgb(241, 250, 140),
            green: Color::Rgb(80, 250, 123),
            teal: Color::Rgb(139, 233, 253),
            sky: Color::Rgb(139, 233, 253),
            sapphire: Color::Rgb(98, 114, 164),
            blue: Color::Rgb(139, 233, 253),
            lavender: Color::Rgb(189, 147, 249),
        };

        Self {
            name: ThemeName::Dracula,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(248, 248, 242),
                subtext1: Color::Rgb(189, 147, 249),
                subtext0: Color::Rgb(98, 114, 164),
                overlay0: Color::Rgb(68, 71, 90),
                surface2: Color::Rgb(68, 71, 90),
                surface1: Color::Rgb(56, 59, 77),
                surface0: Color::Rgb(40, 42, 54),
                border: Color::Rgb(68, 71, 90),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    categorical.sky,
                    categorical.green,
                    categorical.yellow,
                    categorical.peach,
                    categorical.red,
                    categorical.pink,
                    categorical.mauve,
                    categorical.lavender,
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.mauve,
                    medium: categorical.pink,
                    high: categorical.green,
                    empty: Color::Rgb(68, 71, 90),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.pink,
                    midground: categorical.mauve,
                    background: Color::Rgb(68, 71, 90),
                },
                categorical,
            },
        }
    }

    pub fn nord() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(236, 239, 244),
            flamingo: Color::Rgb(216, 222, 233),
            pink: Color::Rgb(191, 97, 106),
            mauve: Color::Rgb(180, 142, 173),
            red: Color::Rgb(191, 97, 106),
            maroon: Color::Rgb(208, 135, 112),
            peach: Color::Rgb(208, 135, 112),
            yellow: Color::Rgb(235, 203, 139),
            green: Color::Rgb(163, 190, 140),
            teal: Color::Rgb(143, 188, 187),
            sky: Color::Rgb(136, 192, 208),
            sapphire: Color::Rgb(129, 161, 193),
            blue: Color::Rgb(94, 129, 172),
            lavender: Color::Rgb(180, 142, 173),
        };

        Self {
            name: ThemeName::Nord,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(236, 239, 244),
                subtext1: Color::Rgb(216, 222, 233),
                subtext0: Color::Rgb(143, 188, 187),
                overlay0: Color::Rgb(76, 86, 106),
                surface2: Color::Rgb(59, 66, 82),
                surface1: Color::Rgb(46, 52, 64),
                surface0: Color::Rgb(43, 48, 59),
                border: Color::Rgb(76, 86, 106),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    categorical.sky,
                    categorical.green,
                    categorical.yellow,
                    categorical.peach,
                    categorical.red,
                    categorical.mauve,
                    categorical.blue,
                    categorical.lavender,
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.blue,
                    medium: categorical.sky,
                    high: categorical.green,
                    empty: Color::Rgb(46, 52, 64),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.sky,
                    midground: categorical.blue,
                    background: Color::Rgb(59, 66, 82),
                },
                categorical,
            },
        }
    }

    pub fn gruvbox_dark() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(235, 219, 178),
            flamingo: Color::Rgb(214, 93, 14),
            pink: Color::Rgb(211, 134, 155),
            mauve: Color::Rgb(211, 134, 155),
            red: Color::Rgb(251, 73, 52),
            maroon: Color::Rgb(204, 36, 29),
            peach: Color::Rgb(254, 128, 25),
            yellow: Color::Rgb(250, 189, 47),
            green: Color::Rgb(184, 187, 38),
            teal: Color::Rgb(142, 192, 124),
            sky: Color::Rgb(131, 165, 152),
            sapphire: Color::Rgb(69, 133, 136),
            blue: Color::Rgb(131, 165, 152),
            lavender: Color::Rgb(214, 93, 14),
        };

        Self {
            name: ThemeName::GruvboxDark,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(235, 219, 178),
                subtext1: Color::Rgb(213, 196, 161),
                subtext0: Color::Rgb(168, 153, 132),
                overlay0: Color::Rgb(124, 111, 100),
                surface2: Color::Rgb(60, 56, 54),
                surface1: Color::Rgb(50, 48, 47),
                surface0: Color::Rgb(40, 40, 40),
                border: Color::Rgb(60, 56, 54),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    categorical.blue,
                    categorical.green,
                    categorical.yellow,
                    categorical.peach,
                    categorical.red,
                    categorical.maroon,
                    categorical.mauve,
                    categorical.pink,
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.blue,
                    medium: categorical.yellow,
                    high: categorical.red,
                    empty: Color::Rgb(60, 56, 54),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.yellow,
                    midground: categorical.blue,
                    background: Color::Rgb(60, 56, 54),
                },
                categorical,
            },
        }
    }

    pub fn tokyo_night() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(192, 202, 245),
            flamingo: Color::Rgb(255, 158, 100),
            pink: Color::Rgb(247, 118, 142),
            mauve: Color::Rgb(187, 154, 247),
            red: Color::Rgb(247, 118, 142),
            maroon: Color::Rgb(255, 158, 100),
            peach: Color::Rgb(255, 158, 100),
            yellow: Color::Rgb(224, 175, 104),
            green: Color::Rgb(158, 206, 106),
            teal: Color::Rgb(125, 207, 255),
            sky: Color::Rgb(125, 207, 255),
            sapphire: Color::Rgb(122, 162, 247),
            blue: Color::Rgb(122, 162, 247),
            lavender: Color::Rgb(187, 154, 247),
        };

        Self {
            name: ThemeName::TokyoNight,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(192, 202, 245),
                subtext1: Color::Rgb(169, 177, 214),
                subtext0: Color::Rgb(86, 95, 137),
                overlay0: Color::Rgb(65, 72, 104),
                surface2: Color::Rgb(41, 46, 66),
                surface1: Color::Rgb(36, 40, 59),
                surface0: Color::Rgb(26, 27, 38),
                border: Color::Rgb(65, 72, 104),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    categorical.sky,
                    categorical.green,
                    categorical.yellow,
                    categorical.peach,
                    categorical.red,
                    categorical.mauve,
                    categorical.blue,
                    categorical.lavender,
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.blue,
                    medium: categorical.sky,
                    high: categorical.red,
                    empty: Color::Rgb(36, 40, 59),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.sky,
                    midground: categorical.blue,
                    background: Color::Rgb(41, 46, 66),
                },
                categorical,
            },
        }
    }

    pub fn one_dark() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(171, 178, 191),
            flamingo: Color::Rgb(209, 154, 102),
            pink: Color::Rgb(198, 120, 221),
            mauve: Color::Rgb(198, 120, 221),
            red: Color::Rgb(224, 108, 117),
            maroon: Color::Rgb(190, 80, 70),
            peach: Color::Rgb(209, 154, 102),
            yellow: Color::Rgb(229, 192, 123),
            green: Color::Rgb(152, 195, 121),
            teal: Color::Rgb(86, 182, 194),
            sky: Color::Rgb(97, 175, 239),
            sapphire: Color::Rgb(97, 175, 239),
            blue: Color::Rgb(97, 175, 239),
            lavender: Color::Rgb(198, 120, 221),
        };

        Self {
            name: ThemeName::OneDark,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(171, 178, 191),
                subtext1: Color::Rgb(146, 150, 165),
                subtext0: Color::Rgb(92, 99, 112),
                overlay0: Color::Rgb(73, 78, 90),
                surface2: Color::Rgb(40, 44, 52),
                surface1: Color::Rgb(33, 37, 43),
                surface0: Color::Rgb(30, 33, 39),
                border: Color::Rgb(73, 78, 90),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    categorical.sky,
                    categorical.green,
                    categorical.yellow,
                    categorical.peach,
                    categorical.red,
                    categorical.pink,
                    categorical.mauve,
                    categorical.lavender,
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.blue,
                    medium: categorical.yellow,
                    high: categorical.red,
                    empty: Color::Rgb(40, 44, 52),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.sky,
                    midground: categorical.blue,
                    background: Color::Rgb(40, 44, 52),
                },
                categorical,
            },
        }
    }

    pub fn solarized_dark() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(131, 148, 150),
            flamingo: Color::Rgb(203, 75, 22),
            pink: Color::Rgb(211, 54, 130),
            mauve: Color::Rgb(108, 113, 196),
            red: Color::Rgb(220, 50, 47),
            maroon: Color::Rgb(203, 75, 22),
            peach: Color::Rgb(203, 75, 22),
            yellow: Color::Rgb(181, 137, 0),
            green: Color::Rgb(133, 153, 0),
            teal: Color::Rgb(42, 161, 152),
            sky: Color::Rgb(38, 139, 210),
            sapphire: Color::Rgb(38, 139, 210),
            blue: Color::Rgb(38, 139, 210),
            lavender: Color::Rgb(108, 113, 196),
        };

        Self {
            name: ThemeName::SolarizedDark,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(131, 148, 150),
                subtext1: Color::Rgb(147, 161, 161),
                subtext0: Color::Rgb(101, 123, 131),
                overlay0: Color::Rgb(88, 110, 117),
                surface2: Color::Rgb(7, 54, 66),
                surface1: Color::Rgb(0, 43, 54),
                surface0: Color::Rgb(0, 33, 44),
                border: Color::Rgb(7, 54, 66),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    categorical.sky,
                    categorical.green,
                    categorical.yellow,
                    categorical.peach,
                    categorical.red,
                    categorical.pink,
                    categorical.mauve,
                    categorical.lavender,
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.blue,
                    medium: categorical.yellow,
                    high: categorical.red,
                    empty: Color::Rgb(7, 54, 66),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.yellow,
                    midground: categorical.blue,
                    background: Color::Rgb(7, 54, 66),
                },
                categorical,
            },
        }
    }

    pub fn monokai() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(248, 248, 242),
            flamingo: Color::Rgb(253, 151, 31),
            pink: Color::Rgb(249, 38, 114),
            mauve: Color::Rgb(174, 129, 255),
            red: Color::Rgb(249, 38, 114),
            maroon: Color::Rgb(204, 102, 119),
            peach: Color::Rgb(253, 151, 31),
            yellow: Color::Rgb(230, 219, 116),
            green: Color::Rgb(166, 226, 46),
            teal: Color::Rgb(102, 217, 239),
            sky: Color::Rgb(102, 217, 239),
            sapphire: Color::Rgb(117, 113, 94),
            blue: Color::Rgb(102, 217, 239),
            lavender: Color::Rgb(174, 129, 255),
        };

        Self {
            name: ThemeName::Monokai,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(248, 248, 242),
                subtext1: Color::Rgb(174, 129, 255),
                subtext0: Color::Rgb(117, 113, 94),
                overlay0: Color::Rgb(73, 72, 62),
                surface2: Color::Rgb(39, 40, 34),
                surface1: Color::Rgb(32, 33, 28),
                surface0: Color::Rgb(27, 28, 24),
                border: Color::Rgb(73, 72, 62),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    categorical.sky,
                    categorical.green,
                    categorical.yellow,
                    categorical.peach,
                    categorical.red,
                    categorical.pink,
                    categorical.mauve,
                    categorical.lavender,
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.blue,
                    medium: categorical.yellow,
                    high: categorical.red,
                    empty: Color::Rgb(39, 40, 34),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.yellow,
                    midground: categorical.blue,
                    background: Color::Rgb(39, 40, 34),
                },
                categorical,
            },
        }
    }

    pub fn everforest_dark() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(211, 198, 170),
            flamingo: Color::Rgb(230, 126, 128),
            pink: Color::Rgb(231, 138, 131),
            mauve: Color::Rgb(215, 153, 33),
            red: Color::Rgb(230, 126, 128),
            maroon: Color::Rgb(229, 152, 117),
            peach: Color::Rgb(229, 152, 117),
            yellow: Color::Rgb(219, 188, 127),
            green: Color::Rgb(167, 192, 128),
            teal: Color::Rgb(131, 192, 146),
            sky: Color::Rgb(127, 187, 179),
            sapphire: Color::Rgb(115, 163, 145),
            blue: Color::Rgb(127, 187, 179),
            lavender: Color::Rgb(214, 153, 182),
        };

        Self {
            name: ThemeName::EverforestDark,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(211, 198, 170),
                subtext1: Color::Rgb(167, 192, 128),
                subtext0: Color::Rgb(133, 147, 138),
                overlay0: Color::Rgb(94, 100, 104),
                surface2: Color::Rgb(59, 69, 71),
                surface1: Color::Rgb(47, 56, 58),
                surface0: Color::Rgb(43, 51, 57),
                border: Color::Rgb(59, 69, 71),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    categorical.sky,
                    categorical.green,
                    categorical.yellow,
                    categorical.peach,
                    categorical.red,
                    categorical.pink,
                    categorical.mauve,
                    categorical.lavender,
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.blue,
                    medium: categorical.yellow,
                    high: categorical.red,
                    empty: Color::Rgb(59, 69, 71),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.yellow,
                    midground: categorical.blue,
                    background: Color::Rgb(59, 69, 71),
                },
                categorical,
            },
        }
    }

    pub fn rose_pine() -> Self {
        let categorical = ThemeCategorical {
            rosewater: Color::Rgb(224, 222, 244),
            flamingo: Color::Rgb(246, 193, 119),
            pink: Color::Rgb(235, 111, 146),
            mauve: Color::Rgb(196, 167, 231),
            red: Color::Rgb(235, 111, 146),
            maroon: Color::Rgb(235, 188, 186),
            peach: Color::Rgb(246, 193, 119),
            yellow: Color::Rgb(246, 193, 119),
            green: Color::Rgb(49, 116, 143),
            teal: Color::Rgb(156, 207, 216),
            sky: Color::Rgb(156, 207, 216),
            sapphire: Color::Rgb(144, 140, 170),
            blue: Color::Rgb(156, 207, 216),
            lavender: Color::Rgb(196, 167, 231),
        };

        Self {
            name: ThemeName::RosePine,
            effects: ThemeEffects::default(),
            semantic: ThemeSemantic {
                text: Color::Rgb(224, 222, 244),
                subtext1: Color::Rgb(144, 140, 170),
                subtext0: Color::Rgb(110, 106, 134),
                overlay0: Color::Rgb(64, 61, 82),
                surface2: Color::Rgb(38, 35, 58),
                surface1: Color::Rgb(31, 29, 46),
                surface0: Color::Rgb(25, 23, 36),
                border: Color::Rgb(64, 61, 82),
                white: Color::White,
            },
            scale: ThemeScale {
                speed: [
                    categorical.sky,
                    categorical.teal,
                    categorical.yellow,
                    categorical.peach,
                    categorical.red,
                    categorical.pink,
                    categorical.mauve,
                    categorical.lavender,
                ],
                ip_hash: categorical_ip_hash(categorical),
                heatmap: ThemeHeatmap {
                    low: categorical.blue,
                    medium: categorical.yellow,
                    high: categorical.red,
                    empty: Color::Rgb(38, 35, 58),
                },
                stream: ThemeStream {
                    inflow: categorical.blue,
                    outflow: categorical.green,
                },
                dust: ThemeDust {
                    foreground: categorical.yellow,
                    midground: categorical.blue,
                    background: Color::Rgb(38, 35, 58),
                },
                categorical,
            },
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::catppuccin_mocha()
    }
}

fn categorical_ip_hash(categorical: ThemeCategorical) -> [Color; 14] {
    [
        categorical.rosewater,
        categorical.flamingo,
        categorical.pink,
        categorical.mauve,
        categorical.red,
        categorical.maroon,
        categorical.peach,
        categorical.yellow,
        categorical.green,
        categorical.teal,
        categorical.sky,
        categorical.sapphire,
        categorical.blue,
        categorical.lavender,
    ]
}
