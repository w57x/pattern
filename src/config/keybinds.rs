mod parser {
    use thiserror::Error;

    use crate::input::Mods;

    #[derive(Error, Debug)]
    pub enum KeysParserError {
        #[error("Invalid keybind")]
        Invalid,
    }

    pub fn parse() -> Result<Key, KeysParserError> {
        todo!("Another day, another parser")
    }

    pub struct Key {
        mods: Mods,
        mouse: Mouse,
    }

    pub enum Mouse {
        Left,
        Right,
        Center,
    }
}
