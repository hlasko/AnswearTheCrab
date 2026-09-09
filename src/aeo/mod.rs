//! Answer-engine optimisation: what makes content get quoted.
//!
//! Everything here is computed locally from research we already hold, so these
//! measures cost nothing per brief and can run on every draft.
//!
//! The direction is set by evidence rather than by SEO habit. Measured across
//! our own briefs, 74% of AI Overview citations are pages already in the
//! organic top 10 and the cited positions cluster at 6-10, so the leverage is
//! in the content of pages that rank respectably but not first. The GEO paper
//! (Aggarwal et al., KDD 2024) tested nine strategies and found quotations,
//! statistics and citations gaining 27-41% visibility, while keyword stuffing
//! scored *below* leaving the page alone. That is why nothing here counts
//! keyword density. See `docs/aeo-geo.md`.

pub mod citations;
pub mod evidence;
pub mod gap;
pub mod intent;
pub mod quotable;
pub mod schema;
pub mod verify;

pub use citations::{check, normalise_domain, CitationCheck};
pub use evidence::{evidence, Evidence};
pub use gap::{topic_gap, GapTopic};
pub use intent::{classify, Intent};
pub use quotable::{quotability, Quotability};
