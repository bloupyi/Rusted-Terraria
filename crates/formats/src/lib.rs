pub mod dotnet;
pub mod install;
pub mod lang;
mod reader;
#[doc(hidden)]
pub mod synth;
pub mod wld;
pub mod xact;
pub mod xnb;

pub use reader::Error;
