//! One retained diagnostic capture, independent of editable feature caches.
use crate::{
    model::archive::{ArchiveCapabilities, NativeArchive},
    validation::archive::{validate_archive, validate_capabilities},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureProblem {
    Capture(String),
    InvalidResult(String),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureStatus {
    Empty,
    Ready,
    Failed(CaptureProblem),
}
pub struct Capture {
    capabilities: ArchiveCapabilities,
    captured: Option<NativeArchive>,
    status: CaptureStatus,
}
impl Capture {
    pub fn new(capabilities: ArchiveCapabilities) -> Result<Self, String> {
        validate_capabilities(&capabilities)?;
        Ok(Self {
            capabilities,
            captured: None,
            status: CaptureStatus::Empty,
        })
    }
    pub fn capabilities(&self) -> &ArchiveCapabilities {
        &self.capabilities
    }
    /// Last successful capture; consult status to distinguish a failed newer attempt.
    pub fn captured(&self) -> Option<&NativeArchive> {
        self.captured.as_ref()
    }
    pub fn status(&self) -> &CaptureStatus {
        &self.status
    }
    pub(crate) fn accept(
        &mut self,
        result: Result<NativeArchive, String>,
    ) -> Result<(), CaptureProblem> {
        let result = result.map_err(CaptureProblem::Capture).and_then(|archive| {
            validate_archive(&self.capabilities, &archive)
                .map_err(CaptureProblem::InvalidResult)?;
            Ok(archive)
        });
        match result {
            Ok(archive) => {
                self.captured = Some(archive);
                self.status = CaptureStatus::Ready;
                Ok(())
            }
            Err(problem) => {
                self.status = CaptureStatus::Failed(problem.clone());
                Err(problem)
            }
        }
    }
}
