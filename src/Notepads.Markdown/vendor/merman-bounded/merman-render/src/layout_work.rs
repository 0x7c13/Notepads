use crate::resources::{OperationWorkError, OperationWorkMeter};
use crate::{Error, Result};
#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
use std::sync::Arc;

/// One renderer-owned adapter shared by the Dagre and ELK lower layout kernels.
///
/// The kernels expose different neutral control traits, but both are owned by the same render
/// operation and must consume one cumulative resource budget. Keeping the concrete adapter here
/// prevents each family from inventing a subtly different sticky-error or overflow mapping.
pub(crate) struct OperationLayoutWorkControl {
    meter: Arc<OperationWorkMeter>,
    rejection: RefCell<Option<OperationWorkError>>,
    #[cfg(test)]
    adapter_work: Cell<usize>,
}

impl OperationLayoutWorkControl {
    pub(crate) fn new(meter: Arc<OperationWorkMeter>) -> Self {
        Self {
            meter,
            rejection: RefCell::new(None),
            #[cfg(test)]
            adapter_work: Cell::new(0),
        }
    }

    pub(crate) fn charge_adapter(&mut self, units: usize) -> Result<()> {
        if let Some(error) = self.rejection.borrow().clone() {
            return Err(error.into());
        }
        #[cfg(test)]
        let next_adapter_work = self.checked_add(self.adapter_work.get(), units)?;
        self.meter.charge(units).map_err(|error| {
            *self.rejection.borrow_mut() = Some(error.clone());
            Error::from(error)
        })?;
        #[cfg(test)]
        self.adapter_work.set(next_adapter_work);
        Ok(())
    }

    pub(crate) fn checked_add(&self, left: usize, right: usize) -> Result<usize> {
        self.meter
            .checkpoint(merman_core::OperationPhase::Layout)
            .map_err(Error::from)?;
        left.checked_add(right)
            .ok_or_else(|| self.record_arithmetic_overflow().into())
    }

    pub(crate) fn checked_mul(&self, left: usize, right: usize) -> Result<usize> {
        self.meter
            .checkpoint(merman_core::OperationPhase::Layout)
            .map_err(Error::from)?;
        left.checked_mul(right)
            .ok_or_else(|| self.record_arithmetic_overflow().into())
    }

    pub(crate) fn record_arithmetic_overflow(&self) -> OperationWorkError {
        if let Some(error) = self.rejection.borrow().clone() {
            return error;
        }
        let error = self.meter.arithmetic_overflow();
        *self.rejection.borrow_mut() = Some(error.clone());
        error
    }

    #[cfg(feature = "layout-elk")]
    pub(crate) fn arithmetic_overflow(&self) -> OperationWorkError {
        self.record_arithmetic_overflow()
    }

    pub(crate) fn map_dugong_error(&mut self, error: impl Into<dugong::LayoutError>) -> Error {
        match error.into() {
            dugong::LayoutError::Work(dugong::WorkError::Interrupted) => {
                let rejection = self.rejection.borrow().clone();
                rejection
                    .map(Error::from)
                    .unwrap_or_else(|| Error::from(self.record_arithmetic_overflow()))
            }
            dugong::LayoutError::Work(dugong::WorkError::ArithmeticOverflow) => {
                Error::from(self.record_arithmetic_overflow())
            }
            error => error.into(),
        }
    }

    #[cfg(feature = "layout-elk")]
    pub(crate) fn map_elk_error(&mut self, error: merman_layout_elk::Error) -> Error {
        self.map_elk_error_with_context(error, "ELK")
    }

    #[cfg(feature = "layout-elk")]
    pub(crate) fn map_elk_error_with_context(
        &mut self,
        error: merman_layout_elk::Error,
        context: &str,
    ) -> Error {
        match error.work_error() {
            Some(merman_layout_elk::WorkError::Interrupted) => {
                let rejection = self.rejection.borrow_mut().take();
                rejection
                    .map(Error::from)
                    .unwrap_or_else(|| Error::from(self.record_arithmetic_overflow()))
            }
            Some(merman_layout_elk::WorkError::ArithmeticOverflow) => {
                Error::from(self.record_arithmetic_overflow())
            }
            None => Error::InvalidModel {
                message: format!("{context} layout failed: {error}"),
            },
        }
    }

    #[cfg(feature = "layout-cytoscape")]
    pub(crate) fn map_manatee_error(&mut self, error: manatee::Error) -> Error {
        match error {
            manatee::Error::WorkFailure(manatee::WorkFailure::Interrupted) => self
                .rejection
                .borrow_mut()
                .take()
                .map(Error::from)
                .unwrap_or_else(|| Error::InvalidModel {
                    message: "manatee work control interrupted without a resource error"
                        .to_string(),
                }),
            manatee::Error::WorkFailure(manatee::WorkFailure::ArithmeticOverflow) => {
                Error::from(self.record_arithmetic_overflow())
            }
            error => Error::InvalidModel {
                message: format!("manatee layout failed: {error}"),
            },
        }
    }

    #[cfg(all(test, feature = "layout-elk"))]
    pub(crate) fn adapter_work(&self) -> usize {
        self.adapter_work.get()
    }
}

pub(crate) type DugongOperationWorkControl = OperationLayoutWorkControl;

#[cfg(feature = "layout-elk")]
pub(crate) type ElkOperationWorkControl = OperationLayoutWorkControl;

impl dugong::WorkControl for OperationLayoutWorkControl {
    fn charge(&mut self, units: usize) -> std::result::Result<(), dugong::WorkError> {
        if self.rejection.borrow().is_some() {
            return Err(dugong::WorkError::Interrupted);
        }
        self.meter.charge(units).map_err(|error| {
            *self.rejection.borrow_mut() = Some(error);
            dugong::WorkError::Interrupted
        })
    }
}

#[cfg(feature = "layout-cytoscape")]
impl manatee::WorkControl for OperationLayoutWorkControl {
    fn check(&mut self, units: usize) -> std::result::Result<(), manatee::WorkFailure> {
        if self.rejection.borrow().is_some() {
            return Err(manatee::WorkFailure::Interrupted);
        }
        self.meter.preflight(units).map_err(|error| {
            *self.rejection.borrow_mut() = Some(error);
            manatee::WorkFailure::Interrupted
        })
    }

    fn charge(&mut self, units: usize) -> std::result::Result<(), manatee::WorkFailure> {
        if self.rejection.borrow().is_some() {
            return Err(manatee::WorkFailure::Interrupted);
        }
        self.meter.charge(units).map_err(|error| {
            *self.rejection.borrow_mut() = Some(error);
            manatee::WorkFailure::Interrupted
        })
    }
}

#[cfg(feature = "layout-elk")]
impl merman_layout_elk::WorkControl for OperationLayoutWorkControl {
    fn check(&mut self, units: usize) -> std::result::Result<(), merman_layout_elk::WorkError> {
        if self.rejection.borrow().is_some() {
            return Err(merman_layout_elk::WorkError::Interrupted);
        }
        self.meter.preflight(units).map_err(|error| {
            *self.rejection.borrow_mut() = Some(error);
            merman_layout_elk::WorkError::Interrupted
        })
    }

    fn charge(&mut self, units: usize) -> std::result::Result<(), merman_layout_elk::WorkError> {
        if self.rejection.borrow().is_some() {
            return Err(merman_layout_elk::WorkError::Interrupted);
        }
        self.meter.charge(units).map_err(|error| {
            *self.rejection.borrow_mut() = Some(error);
            merman_layout_elk::WorkError::Interrupted
        })
    }
}
