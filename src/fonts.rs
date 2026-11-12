pub const FONTS: &[(&str, &[u8])] = &[
    (
        "inter-400.woff2",
        include_bytes!("../assets/fonts/inter-400.woff2"),
    ),
    (
        "inter-500.woff2",
        include_bytes!("../assets/fonts/inter-500.woff2"),
    ),
    (
        "inter-600.woff2",
        include_bytes!("../assets/fonts/inter-600.woff2"),
    ),
    (
        "inter-700.woff2",
        include_bytes!("../assets/fonts/inter-700.woff2"),
    ),
    (
        "inter-800.woff2",
        include_bytes!("../assets/fonts/inter-800.woff2"),
    ),
    (
        "mono-400.woff2",
        include_bytes!("../assets/fonts/mono-400.woff2"),
    ),
    (
        "mono-500.woff2",
        include_bytes!("../assets/fonts/mono-500.woff2"),
    ),
    (
        "mono-600.woff2",
        include_bytes!("../assets/fonts/mono-600.woff2"),
    ),
    (
        "mono-700.woff2",
        include_bytes!("../assets/fonts/mono-700.woff2"),
    ),
];

pub fn font(name: &str) -> Option<&'static [u8]> {
    FONTS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}
