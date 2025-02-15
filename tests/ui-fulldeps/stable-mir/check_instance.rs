// run-pass
//! Test that users are able to use stable mir APIs to retrieve monomorphized instances

// ignore-stage1
// ignore-cross-compile
// ignore-remote
// ignore-windows-gnu mingw has troubles with linking https://github.com/rust-lang/rust/pull/116837
// edition: 2021

#![feature(rustc_private)]
#![feature(assert_matches)]
#![feature(control_flow_enum)]

extern crate rustc_middle;
#[macro_use]
extern crate rustc_smir;
extern crate rustc_driver;
extern crate rustc_interface;
extern crate stable_mir;

use mir::{mono::Instance, TerminatorKind::*};
use rustc_middle::ty::TyCtxt;
use rustc_smir::rustc_internal;
use stable_mir::ty::{RigidTy, TyKind};
use stable_mir::*;
use std::io::Write;
use std::ops::ControlFlow;

const CRATE_NAME: &str = "input";

/// This function uses the Stable MIR APIs to get information about the test crate.
fn test_stable_mir(_tcx: TyCtxt<'_>) -> ControlFlow<()> {
    let items = stable_mir::all_local_items();

    // Get all items and split generic vs monomorphic items.
    let (generic, mono): (Vec<_>, Vec<_>) =
        items.into_iter().partition(|item| item.requires_monomorphization());
    assert_eq!(mono.len(), 3, "Expected 2 mono functions and one constant");
    assert_eq!(generic.len(), 2, "Expected 2 generic functions");

    // For all monomorphic items, get the correspondent instances.
    let instances = mono
        .iter()
        .filter_map(|item| mir::mono::Instance::try_from(*item).ok())
        .collect::<Vec<mir::mono::Instance>>();
    assert_eq!(instances.len(), mono.len());

    // For all generic items, try_from should fail.
    assert!(generic.iter().all(|item| mir::mono::Instance::try_from(*item).is_err()));

    for instance in instances {
        test_body(instance.body())
    }
    ControlFlow::Continue(())
}

/// Inspect the instance body
fn test_body(body: mir::Body) {
    for term in body.blocks.iter().map(|bb| &bb.terminator) {
        match &term.kind {
            Call { func, .. } => {
                let TyKind::RigidTy(ty) = func.ty(&body.locals).kind() else { unreachable!() };
                let RigidTy::FnDef(def, args) = ty else { unreachable!() };
                let result = Instance::resolve(def, &args);
                assert!(result.is_ok());
            }
            Goto { .. } | Assert { .. } | SwitchInt { .. } | Return | Drop { .. } => {
                /* Do nothing */
            }
            _ => {
                unreachable!("Unexpected terminator {term:?}")
            }
        }
    }
}

/// This test will generate and analyze a dummy crate using the stable mir.
/// For that, it will first write the dummy crate into a file.
/// Then it will create a `StableMir` using custom arguments and then
/// it will run the compiler.
fn main() {
    let path = "instance_input.rs";
    generate_input(&path).unwrap();
    let args = vec![
        "rustc".to_string(),
        "-Cpanic=abort".to_string(),
        "--crate-type=lib".to_string(),
        "--crate-name".to_string(),
        CRATE_NAME.to_string(),
        path.to_string(),
    ];
    run!(args, tcx, test_stable_mir(tcx)).unwrap();
}

fn generate_input(path: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    write!(
        file,
        r#"
    pub fn ty_param<T>(t: &T) -> T where T: Clone {{
        t.clone()
    }}

    pub fn const_param<const LEN: usize>(a: [bool; LEN]) -> bool {{
        LEN > 0 && a[0]
    }}

    pub fn monomorphic() {{
        let v = vec![10];
        let dup = ty_param(&v);
        assert_eq!(v, dup);
    }}

    pub mod foo {{
        pub fn bar_mono(i: i32) -> i64 {{
            i as i64
        }}
    }}
    "#
    )?;
    Ok(())
}
