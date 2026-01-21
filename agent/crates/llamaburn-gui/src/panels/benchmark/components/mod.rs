mod model_selector;
mod multi_select;
mod transport;

// Widget-based API (preferred)
pub use model_selector::{ModelSelector, ModelSelectorResponse};
pub use transport::{TransportControls, TransportResponse};

// Legacy function-based API (for backwards compatibility)
pub use model_selector::render_model_selector;
pub use multi_select::{multi_select_dropdown, toggle_selection};
