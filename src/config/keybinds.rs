use crate::input::Mods;
use xkbcommon::xkb;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum KeySpec {
    Keysym(u32),
    Keycode(u32),
    Mouse(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyPattern {
    pub mods: Mods,
    pub key: KeySpec,
}

pub fn parse_keybind(chord: &str) -> Option<KeyPattern> {
    let mut mods = Mods::default();
    let mut key_name = None;

    let parts: Vec<&str> = chord.split('+').map(|s| s.trim()).collect();
    for part in parts {
        let upper = part.to_uppercase();
        match upper.as_str() {
            "CTRL" | "CONTROL" => mods.ctrl = true,
            "ALT" | "MOD1" => mods.alt = true,
            "SHIFT" => mods.shift = true,
            "SUPER" | "LOGO" | "WIN" | "MOD4" => mods.mod4 = true,
            _ => {
                key_name = Some(part);
            }
        }
    }

    let key_name = key_name?;
    let key = if key_name.to_lowercase().starts_with("code:") {
        let code_str = &key_name[5..];
        if let Ok(code) = code_str.parse::<u32>() {
            KeySpec::Keycode(code)
        } else {
            return None;
        }
    } else if key_name.to_lowercase().starts_with("mouse:") {
        let mouse_str = &key_name[6..];
        if let Ok(btn) = mouse_str.parse::<u32>() {
            KeySpec::Mouse(btn)
        } else {
            return None;
        }
    } else {
        let keysym = xkb::keysym_from_name(key_name, xkb::KEYSYM_CASE_INSENSITIVE);
        if keysym.raw() == xkb::keysyms::KEY_NoSymbol {
            return None;
        }
        let lower_keysym = keysym_to_lower(keysym);
        KeySpec::Keysym(lower_keysym.raw())
    };

    Some(KeyPattern { mods, key })
}

pub fn keysym_to_lower(keysym: xkb::Keysym) -> xkb::Keysym {
    let name = xkb::keysym_get_name(keysym);
    let lower_name = name.to_lowercase();
    let lower_keysym = xkb::keysym_from_name(&lower_name, xkb::KEYSYM_CASE_INSENSITIVE);
    if lower_keysym.raw() == xkb::keysyms::KEY_NoSymbol {
        keysym
    } else {
        lower_keysym
    }
}
