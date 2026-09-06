//! The fonts Roblox ships, as `font-<name>` classes: every family behind
//! `Enum.Font`, plus the weighted items as aliases, `font-gotham-bold`.

/// One font class: the class name, the family file, the weight, and
/// whether it is italic.
pub struct FontEntry {
    pub class: &'static str,
    pub family: &'static str,
    pub weight: &'static str,
    pub italic: bool,
}

const fn font(class: &'static str, family: &'static str) -> FontEntry {
    FontEntry {
        class,
        family,
        weight: "Regular",
        italic: false,
    }
}

const fn weighted(class: &'static str, family: &'static str, weight: &'static str) -> FontEntry {
    FontEntry {
        class,
        family,
        weight,
        italic: false,
    }
}

/// The families, each as the enum names it; the JSON name is the
/// family file under `rbxasset://fonts/families/`.
pub const FONTS: &[FontEntry] = &[
    font("legacy", "LegacyArial"),
    font("arial", "Arial"),
    weighted("arial-bold", "Arial", "Bold"),
    font("source-sans", "SourceSansPro"),
    weighted("source-sans-bold", "SourceSansPro", "Bold"),
    weighted("source-sans-semibold", "SourceSansPro", "SemiBold"),
    weighted("source-sans-light", "SourceSansPro", "Light"),
    FontEntry {
        class: "source-sans-italic",
        family: "SourceSansPro",
        weight: "Regular",
        italic: true,
    },
    font("bodoni", "AccanthisADFStd"),
    font("garamond", "Guru"),
    font("cartoon", "ComicNeueAngular"),
    font("code", "Inconsolata"),
    font("highway", "HighwayGothic"),
    font("sci-fi", "Zekton"),
    font("arcade", "PressStart2P"),
    font("fantasy", "Balthazar"),
    font("antique", "RomanAntique"),
    font("gotham", "GothamSSm"),
    weighted("gotham-medium", "GothamSSm", "Medium"),
    weighted("gotham-bold", "GothamSSm", "Bold"),
    weighted("gotham-black", "GothamSSm", "Heavy"),
    font("amatic-sc", "AmaticSC"),
    font("bangers", "Bangers"),
    font("creepster", "Creepster"),
    font("denk-one", "DenkOne"),
    font("fondamento", "Fondamento"),
    font("fredoka-one", "FredokaOne"),
    font("grenze-gotisch", "GrenzeGotisch"),
    font("indie-flower", "IndieFlower"),
    font("josefin-sans", "JosefinSans"),
    font("jura", "Jura"),
    font("kalam", "Kalam"),
    font("luckiest-guy", "LuckiestGuy"),
    font("merriweather", "Merriweather"),
    font("michroma", "Michroma"),
    font("nunito", "Nunito"),
    font("oswald", "Oswald"),
    font("patrick-hand", "PatrickHand"),
    font("permanent-marker", "PermanentMarker"),
    font("roboto", "Roboto"),
    font("roboto-condensed", "RobotoCondensed"),
    font("roboto-mono", "RobotoMono"),
    font("sarpanch", "Sarpanch"),
    font("special-elite", "SpecialElite"),
    font("titillium-web", "TitilliumWeb"),
    font("ubuntu", "Ubuntu"),
    font("builder-sans", "BuilderSans"),
    weighted("builder-sans-medium", "BuilderSans", "Medium"),
    weighted("builder-sans-bold", "BuilderSans", "Bold"),
    weighted("builder-sans-extra-bold", "BuilderSans", "ExtraBold"),
    font("montserrat", "Montserrat"),
    font("arimo", "Arimo"),
];

/// The font class named, `gotham-bold`, with its family path.
pub fn lookup(name: &str) -> Option<&'static FontEntry> {
    FONTS.iter().find(|f| f.class == name)
}

/// The asset path of a family file.
pub fn family_path(family: &str) -> String {
    format!("rbxasset://fonts/families/{family}.json")
}
