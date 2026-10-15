//! PinIt core. Pure domain logic and ports. All geometry is integer *physical* pixels
//! of the virtual desktop (one linear space across mixed-DPI monitors); user-facing
//! sizes are DIP and are multiplied by the target display scale before placement.
pub mod geometry;
pub mod image_ref;
pub mod image_size;
pub mod placement;
pub mod ports;
pub mod resolver;
pub mod rng;
pub mod service;
pub mod view_state;
