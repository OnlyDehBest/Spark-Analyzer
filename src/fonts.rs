pub const FONTS: &[(&str, &[u8])] = &[
    ("inter.woff2", include_bytes!("../assets/fonts/inter.woff2")),
    ("mono.woff2", include_bytes!("../assets/fonts/mono.woff2")),
];

pub fn font(name: &str) -> Option<&'static [u8]> {
    FONTS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}
