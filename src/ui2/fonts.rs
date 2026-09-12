// Typography: Inter for interface copy and JetBrains Mono for technical
// metadata, measurements, and state labels.

use iced::Font;

pub const INTER_REGULAR: Font = Font::with_name("Inter");
pub const INTER_SEMIBOLD: Font = Font::with_name("Inter SemiBold");
pub const JETBRAINS_MONO: Font = Font::with_name("JetBrains Mono");
pub const JETBRAINS_MONO_BOLD: Font = Font::with_name("JetBrains Mono Bold");

pub const INTER_REGULAR_DATA: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
pub const INTER_SEMIBOLD_DATA: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");
pub const JETBRAINS_MONO_DATA: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
pub const JETBRAINS_MONO_BOLD_DATA: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf");

pub fn load() -> Vec<Font> {
    vec![
        INTER_REGULAR,
        INTER_SEMIBOLD,
        JETBRAINS_MONO,
        JETBRAINS_MONO_BOLD,
    ]
}
