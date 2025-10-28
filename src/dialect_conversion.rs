//! Dialect conversion infrastructure.
//!
//! This module provides conversion infrastructure similar to MLIR's dialect conversion,
//! allowing transformations of operations from one dialect to another through pattern matching.

use rustc_hash::FxHashMap;
use thiserror::Error;

use crate::context::Context;
use crate::context::Ptr;
use crate::dialect::{Dialect, DialectName};
use crate::op::{Op, OpId};
use crate::operation::Operation;
use crate::operation::{WalkOrder, WalkResult};
use crate::pattern_match::{AccumulatingListener, GenericPatternRewriter};
use crate::pattern_match::{PatternRewriter, PatternRewriterError};
use crate::printable::Printable;
use crate::rewrite::{PatternApplicator, RewritePatternSet};

/// This enumeration corresponds to the specific action to take when
/// considering an operation legal for this conversion target.
pub enum LegalizationAction {
    /// The target supports this operation.
    Legal,

    /// This operation has dynamic legalization constraints that must be checked
    /// by the target.
    Dynamic,

    /// The target explicitly does not support this operation.
    Illegal,
}

#[derive(Default)]
pub struct ConversionTarget {
    legal_dialects: FxHashMap<DialectName, LegalizationAction>,
}

impl ConversionTarget {
    // Note: This is currently incomplete and will be implemented in future work
    #[allow(dead_code)]
    fn set_op_action(&mut self, _op_id: OpId, _action: LegalizationAction) {
        todo!("Dynamic operation-level legalization not yet implemented")
    }

    pub fn add_legal_dialect(&mut self, dialect: &Dialect) {
        self.legal_dialects
            .entry(dialect.name.clone())
            .or_insert(LegalizationAction::Legal);
    }
    pub fn add_illegal_dialect(&mut self, dialect: &Dialect) {
        self.legal_dialects
            .entry(dialect.name.clone())
            .or_insert(LegalizationAction::Illegal);
    }
    /// Add a dynamically legal operation type.
    /// 
    /// **Note**: Dynamic legalization is not yet fully implemented.
    /// This method is provided for API compatibility but will panic if called.
    /// Use `add_legal_dialect` or `add_illegal_dialect` for now.
    #[allow(dead_code)]
    pub fn add_dynamically_legal_op<OpT: Op>(&mut self, _callback: fn(Ptr<Operation>) -> bool) {
        // TODO: Store the callback in ConversionTarget and use it during is_legal checks
        self.set_op_action(OpT::get_opid_static(), LegalizationAction::Dynamic);
        todo!("Dynamic operation-level legalization not yet implemented - use dialect-level legalization instead");
    }

    pub fn is_legal(&self, ctx: &Context, op: Ptr<Operation>) -> LegalOpDetails {
        let dialect_name = &op.deref(ctx).get_opid().dialect;
        match self.legal_dialects.get(dialect_name) {
            Some(legal_action) => match legal_action {
                LegalizationAction::Legal => LegalOpDetails::Legal {
                    is_recursively_legal: false,
                },
                LegalizationAction::Dynamic => {
                    // TODO: Use stored callback to determine legality
                    todo!("Dynamic legalization not yet implemented")
                }
                LegalizationAction::Illegal => LegalOpDetails::Illegal,
            },
            None => LegalOpDetails::Unknown,
        }
    }
}

/// A structure containing additional information describing a specific legal
/// operation instance.
#[derive(PartialEq, Eq)]
pub enum LegalOpDetails {
    Illegal,
    Legal {
        /// A flag that indicates if this operation is 'recursively' legal. This
        /// means that if an operation is legal, either statically or dynamically,
        /// all of the operations nested within are also considered legal.
        is_recursively_legal: bool,
    },
    Unknown,
}

#[derive(Debug, Error)]
enum LegalizationError {
    #[error("Legalization failed. RewritePattern error: {0}")]
    RewritePatternError(#[from] PatternRewriterError),
    #[error("Legalization failed: {msg}")]
    Failure { msg: String },
}

struct OperationLegalizer {
    target: ConversionTarget,
    applicator: PatternApplicator,
}

impl OperationLegalizer {
    fn new(target: ConversionTarget, patterns: RewritePatternSet) -> Self {
        Self {
            target,
            applicator: PatternApplicator::new(patterns),
        }
    }

    fn legalize(
        &self,
        ctx: &mut Context,
        op: Ptr<Operation>,
        rewriter: &mut dyn PatternRewriter,
    ) -> Result<(), LegalizationError> {
        match self.target.is_legal(ctx, op) {
            LegalOpDetails::Illegal => (),
            LegalOpDetails::Legal {
                is_recursively_legal,
            } => {
                if is_recursively_legal {
                    // TODO: If this operation is recursively legal, mark its children as ignored so
                    // that we don't consider them for legalization.
                }
                return Ok(());
            }
            LegalOpDetails::Unknown => (),
        };
        // TODO: Check to see if the operation is ignored and doesn't need to be converted.
        self.legalize_with_pattern(ctx, op, rewriter)
    }

    fn legalize_with_pattern(
        &self,
        ctx: &mut Context,
        op: Ptr<Operation>,
        rewriter: &mut dyn PatternRewriter,
    ) -> Result<(), LegalizationError> {
        if !self.applicator.match_and_rewrite(ctx, op, rewriter)? {
            return Err(LegalizationError::Failure {
                msg: format!(
                    "applicator failed to match any pattern on operation: {}",
                    op.deref(ctx).disp(ctx)
                ),
            });
        }
        Ok(())
    }

    /// Check if the given operation is set as legal for this target.
    pub fn is_legal(&self, ctx: &Context, op: Ptr<Operation>) -> bool {
        matches!(self.target.is_legal(ctx, op), LegalOpDetails::Legal { .. })
    }

    /// Check if the given operation is set as illegal for this target.
    pub fn is_illegal(&self, ctx: &Context, op: Ptr<Operation>) -> bool {
        self.target.is_legal(ctx, op) == LegalOpDetails::Illegal
    }
}

/// Apply rewrite patterns to the op and all it's nested operations until there is no
/// illegal operations left as set by the conversion target.
pub fn apply_partial_conversion(
    ctx: &mut Context,
    op: Ptr<Operation>,
    target: ConversionTarget,
    pattern_set: RewritePatternSet,
) -> Result<(), ConversionError> {
    let op_convertor = OperationConverter::new(target, pattern_set, OpConversionMode::Partial);
    op_convertor.convert_operations(ctx, vec![op])?;
    // TODO: legalize newly inserted/replaced operations
    Ok(())
}

/// Conversion modes for the operation converter.
/// 
/// **Note**: Only `Partial` and `Full` modes are currently implemented.
pub enum OpConversionMode {
    /// In this mode, the conversion will ignore failed conversions to allow
    /// illegal operations to co-exist in the IR.
    Partial,

    /// In this mode, all operations must be legal for the given target for the
    /// conversion to succeed.
    Full,

    // Analysis mode is not yet implemented
    // /// In this mode, operations are analyzed for legality. No actual rewrites are
    // /// applied to the operations on success.
    // Analysis,
}

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error("Conversion failed. Failed to legalize {failed_op} with error: {orig_error_msg}")]
    FailedToLegalize {
        orig_error_msg: String,
        failed_op: String,
    },
}

struct OperationConverter {
    legalizer: OperationLegalizer,
    mode: OpConversionMode,
}

impl OperationConverter {
    fn new(
        target: ConversionTarget,
        pattern_set: RewritePatternSet,
        mode: OpConversionMode,
    ) -> Self {
        Self {
            legalizer: OperationLegalizer::new(target, pattern_set),
            mode,
        }
    }

    fn convert_operations(
        &self,
        ctx: &mut Context,
        ops: Vec<Ptr<Operation>>,
    ) -> Result<(), ConversionError> {
        let target = &self.legalizer.target;
        let mut to_convert: Vec<Ptr<Operation>> = vec![];
        for op in ops {
            op.walk(ctx, WalkOrder::PreOrder, &mut |op| {
                to_convert.push(op);
                match target.is_legal(ctx, op) {
                    LegalOpDetails::Legal {
                        is_recursively_legal,
                    } if is_recursively_legal => WalkResult::Skip,
                    _ => WalkResult::Advance,
                }
            });
        }
        let listener = Box::<AccumulatingListener>::default();
        let mut rewriter = GenericPatternRewriter::new(Some(listener));
        for op in to_convert.into_iter() {
            if rewriter
                .get_listener()
                .unwrap()
                .get_erased_ops()
                .contains(&op)
            {
                continue;
            }
            if self.legalizer.is_legal(ctx, op) {
                continue;
            }
            self.convert(ctx, op, &mut rewriter)?;
        }

        Ok(())
    }

    fn convert(
        &self,
        ctx: &mut Context,
        op: Ptr<Operation>,
        rewriter: &mut dyn PatternRewriter,
    ) -> Result<(), ConversionError> {
        let res = self.legalizer.legalize(ctx, op, rewriter);
        if let Err(err) = res {
            match self.mode {
                // Partial conversions allow conversions to fail if the operation was not
                // explicitly marked as illegal.
                OpConversionMode::Partial => {
                    if self.legalizer.is_illegal(ctx, op) {
                        return Err(ConversionError::FailedToLegalize {
                            orig_error_msg: err.to_string(),
                            failed_op: op.deref(ctx).disp(ctx).to_string(),
                        });
                    }
                }
                OpConversionMode::Full => Err(ConversionError::FailedToLegalize {
                    orig_error_msg: err.to_string(),
                    failed_op: op.deref(ctx).disp(ctx).to_string(),
                })?,
            }
        }
        Ok(())
    }
}
