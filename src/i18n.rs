use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize)]
pub enum Lang {
    #[default]
    It,
    En,
}

impl Lang {
    pub fn parse(s: &str) -> Lang {
        if s.trim().to_ascii_lowercase().starts_with("it") {
            Lang::It
        } else {
            Lang::En
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Lang::It => "it",
            Lang::En => "en",
        }
    }
}

thread_local! {
    static LANG: Cell<Lang> = const { Cell::new(Lang::It) };
}

pub fn set_lang(l: Lang) {
    LANG.with(|c| c.set(l));
}

pub fn lang() -> Lang {
    LANG.with(|c| c.get())
}

pub fn is_en() -> bool {
    lang() == Lang::En
}

pub fn t(it: &'static str, en: &'static str) -> &'static str {
    if is_en() { en } else { it }
}

pub struct LangGuard(Lang);

impl Drop for LangGuard {
    fn drop(&mut self) {
        set_lang(self.0);
    }
}

pub fn scoped(l: Lang) -> LangGuard {
    let prev = lang();
    set_lang(l);
    LangGuard(prev)
}

#[macro_export]
macro_rules! tf {
    ($it:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_en() { format!($en $(, $arg)*) } else { format!($it $(, $arg)*) }
    };
}
