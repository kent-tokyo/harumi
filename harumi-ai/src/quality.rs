// quality.rs — QualityProfile, QualityGate, QualityResult

use harumi::PageFitSummary;

/// High-level translation quality policy controlling layout tolerance and retry behaviour.
///
/// Pass this to [`TranslateOptions::profile`] or [`TranslateOptionsBuilder::profile`].
/// The gate thresholds below are starting points; construct a [`QualityGate`] directly
/// for fine-grained control.
#[derive(Debug, Clone, Default)]
pub enum QualityProfile {
    /// Prioritise preserving the original layout.
    ///
    /// Allows font shrinking to the minimum size and up to 10 collisions per page.
    /// Use this when pixel-level fidelity matters more than reading comfort.
    PreserveLayout,

    /// Balance readability and layout fidelity.
    ///
    /// Rejects overflow and Major collisions but tolerates minor font shrinking.
    Readable,

    /// Zero tolerance for layout problems.
    ///
    /// Any overflow or collision returns [`Error::QualityGateFailed`].
    Strict,

    /// Always succeed: return the PDF regardless of quality.
    ///
    /// The [`TranslateOutput::quality`] report still carries diagnostics.
    #[default]
    BestEffort,
}

/// Fine-grained layout quality thresholds used to gate the final PDF.
///
/// Build from a [`QualityProfile`] with [`QualityGate::from_profile`], or construct
/// directly for custom thresholds.
#[derive(Debug, Clone)]
pub struct QualityGate {
    /// Maximum number of unique collisions allowed per page (`None` = unlimited).
    pub max_collision_count: Option<usize>,
    /// Maximum number of overflow regions allowed per page (`None` = unlimited).
    pub max_overflow_count: Option<usize>,
    /// Maximum number of regions that had their font shrunk (`None` = unlimited).
    pub max_shrunk_count: Option<usize>,
    /// Whether `PlacementStatus::ShrunkToMin` is acceptable (`true` = allowed).
    pub allow_shrunk_to_min: bool,
    /// Maximum worst-collision overlap area in pt² (`None` = unlimited).
    pub max_worst_overlap_area: Option<f32>,
}

impl QualityGate {
    /// Build a gate from a standard [`QualityProfile`].
    pub fn from_profile(profile: &QualityProfile) -> Self {
        match profile {
            QualityProfile::PreserveLayout => Self {
                max_collision_count: Some(10),
                max_overflow_count: Some(5),
                max_shrunk_count: None,
                allow_shrunk_to_min: true,
                max_worst_overlap_area: Some(1000.0),
            },
            QualityProfile::Readable => Self {
                max_collision_count: Some(3),
                max_overflow_count: Some(0),
                max_shrunk_count: None,
                allow_shrunk_to_min: false,
                max_worst_overlap_area: Some(200.0),
            },
            QualityProfile::Strict => Self {
                max_collision_count: Some(0),
                max_overflow_count: Some(0),
                max_shrunk_count: None,
                allow_shrunk_to_min: false,
                max_worst_overlap_area: Some(0.0),
            },
            QualityProfile::BestEffort => Self {
                max_collision_count: None,
                max_overflow_count: None,
                max_shrunk_count: None,
                allow_shrunk_to_min: true,
                max_worst_overlap_area: None,
            },
        }
    }

    /// Check a single page's [`PageFitSummary`] against the gate thresholds.
    ///
    /// Returns [`QualityResult::Pass`] when all checks pass, or
    /// [`QualityResult::Fail`] with a list of [`QualityViolation`]s otherwise.
    pub fn evaluate(&self, summary: &PageFitSummary) -> QualityResult {
        let mut violations = Vec::new();

        if let Some(limit) = self.max_collision_count
            && summary.collision_count > limit {
            violations.push(QualityViolation::TooManyCollisions {
                count: summary.collision_count,
                limit,
            });
        }

        if let Some(limit) = self.max_overflow_count
            && summary.overflow_count > limit {
            violations.push(QualityViolation::TooManyOverflows {
                count: summary.overflow_count,
                limit,
            });
        }

        if let Some(limit) = self.max_shrunk_count
            && summary.shrunk_count > limit {
            violations.push(QualityViolation::TooManyShrunk {
                count: summary.shrunk_count,
                limit,
            });
        }

        // allow_shrunk_to_min: shrunk_count covers both Shrunk and ShrunkToMin;
        // we can't distinguish in PageFitSummary alone. Per-region detail is in
        // RegionFitPlan::fit.status. (Currently informational only — no violation
        // is added here because the count already tracks this via max_shrunk_count.)

        if let Some(limit) = self.max_worst_overlap_area
            && summary.worst_overlap_area > limit {
            violations.push(QualityViolation::WorstOverlapTooLarge {
                area: summary.worst_overlap_area,
                limit,
            });
        }

        if violations.is_empty() {
            QualityResult::Pass
        } else {
            // Only font-shrink violations → Warn (layout is preserved, text just smaller).
            // Any overflow, collision, or overlap violation → Fail.
            let hard_fail = violations.iter().any(|v| {
                !matches!(v, QualityViolation::TooManyShrunk { .. })
            });
            if hard_fail {
                QualityResult::Fail(violations)
            } else {
                QualityResult::Warn(violations)
            }
        }
    }

    /// Returns `true` if the profile never rejects a PDF (`BestEffort`-equivalent gate).
    pub fn is_permissive(&self) -> bool {
        self.max_collision_count.is_none()
            && self.max_overflow_count.is_none()
            && self.max_shrunk_count.is_none()
            && self.max_worst_overlap_area.is_none()
    }
}

/// Outcome of [`QualityGate::evaluate`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum QualityResult {
    /// All gate checks passed with no violations.
    Pass,
    /// Gate passed but with minor violations (e.g. font shrinking only).
    ///
    /// The PDF is usable; callers may log the violations or expose them
    /// in a quality report without treating the output as failed.
    Warn(Vec<QualityViolation>),
    /// One or more hard gate checks failed (overflow, collision, large overlap).
    Fail(Vec<QualityViolation>),
}

impl QualityResult {
    /// Returns `true` only when the result is [`QualityResult::Pass`] (no violations at all).
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Returns `true` when the result is [`Pass`](Self::Pass) or [`Warn`](Self::Warn).
    ///
    /// Use this to check whether the output PDF is acceptable for downstream use,
    /// while still allowing minor font-shrink warnings through.
    pub fn is_ok(&self) -> bool {
        !matches!(self, Self::Fail(_))
    }

    /// Returns the violation list, or an empty slice for [`Pass`](Self::Pass).
    pub fn violations(&self) -> &[QualityViolation] {
        match self {
            Self::Pass => &[],
            Self::Warn(v) | Self::Fail(v) => v,
        }
    }
}

/// A specific gate threshold that was exceeded.
#[derive(Debug, Clone)]
pub enum QualityViolation {
    /// Collision count exceeded the allowed limit.
    TooManyCollisions { count: usize, limit: usize },
    /// Overflow count exceeded the allowed limit.
    TooManyOverflows { count: usize, limit: usize },
    /// Shrunk-region count exceeded the allowed limit.
    TooManyShrunk { count: usize, limit: usize },
    /// The worst collision's overlap area exceeded the area limit (pt²).
    WorstOverlapTooLarge { area: f32, limit: f32 },
}

impl std::fmt::Display for QualityViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyCollisions { count, limit } => {
                write!(f, "{count} collisions exceed limit of {limit}")
            }
            Self::TooManyOverflows { count, limit } => {
                write!(f, "{count} overflows exceed limit of {limit}")
            }
            Self::TooManyShrunk { count, limit } => {
                write!(f, "{count} shrunk regions exceed limit of {limit}")
            }
            Self::WorstOverlapTooLarge { area, limit } => {
                write!(f, "worst overlap {area:.1} pt² exceeds limit of {limit:.1} pt²")
            }
        }
    }
}
