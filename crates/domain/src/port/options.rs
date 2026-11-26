use std::collections::HashMap;
use std::io;

use crate::Options;

// Where what each agent offered last is kept, so the picker has it before the agent answers.
pub trait OptionStore {
    fn load(&self) -> io::Result<HashMap<String, Options>>;
    fn save(&self, known: &HashMap<String, Options>) -> io::Result<()>;
}
