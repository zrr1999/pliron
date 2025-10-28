//! Test for conversion infrastructure from PR #4

mod common;
use common::*;

use pliron::op::Op;
use pliron::operation::{WalkOrder, WalkResult};
use pliron::result::Result;

#[test]
fn test_walk_preorder() -> Result<()> {
    let mut ctx = setup_context_dialects();
    let (module_op, _, _, _) = const_ret_in_mod(&mut ctx)?;
    let mut ops_visited = vec![];
    
    module_op.get_operation().walk(
        &ctx,
        WalkOrder::PreOrder,
        &mut |op| {
            ops_visited.push(op);
            WalkResult::Advance
        },
    );
    
    // Should have visited module, func, constant and return ops
    assert!(ops_visited.len() >= 4);
    Ok(())
}

#[test]
fn test_walk_postorder() -> Result<()> {
    let mut ctx = setup_context_dialects();
    let (module_op, _, _, _) = const_ret_in_mod(&mut ctx)?;
    let mut ops_visited = vec![];
    
    module_op.get_operation().walk(
        &ctx,
        WalkOrder::PostOrder,
        &mut |op| {
            ops_visited.push(op);
            WalkResult::Advance
        },
    );
    
    // Should have visited module, func, constant and return ops
    assert!(ops_visited.len() >= 4);
    Ok(())
}

#[test]
fn test_walk_only() -> Result<()> {
    let mut ctx = setup_context_dialects();
    let (module_op, _, _, _) = const_ret_in_mod(&mut ctx)?;
    let mut const_ops = vec![];
    
    module_op.get_operation().walk_only::<ConstantOp>(
        &ctx,
        WalkOrder::PreOrder,
        &mut |const_op| {
            const_ops.push(const_op.get_operation());
            WalkResult::Advance
        },
    );
    
    assert_eq!(const_ops.len(), 1);
    Ok(())
}

#[test]
fn test_walk_interrupt() -> Result<()> {
    let mut ctx = setup_context_dialects();
    let (module_op, _, _, _) = const_ret_in_mod(&mut ctx)?;
    let mut ops_visited = vec![];
    
    module_op.get_operation().walk(
        &ctx,
        WalkOrder::PreOrder,
        &mut |op| {
            ops_visited.push(op);
            if ops_visited.len() >= 2 {
                WalkResult::Interrupt
            } else {
                WalkResult::Advance
            }
        },
    );
    
    // Should have stopped after visiting 2 ops
    assert_eq!(ops_visited.len(), 2);
    Ok(())
}

#[test]
fn test_walk_skip() -> Result<()> {
    let mut ctx = setup_context_dialects();
    let (module_op, _, _, _) = const_ret_in_mod(&mut ctx)?;
    let mut ops_visited = vec![];
    
    module_op.get_operation().walk(
        &ctx,
        WalkOrder::PreOrder,
        &mut |op| {
            ops_visited.push(op);
            // Skip after first op (module), so we don't visit nested ops
            if ops_visited.len() == 1 {
                WalkResult::Skip
            } else {
                WalkResult::Advance
            }
        },
    );
    
    // Should have only visited the module op
    assert_eq!(ops_visited.len(), 1);
    Ok(())
}

