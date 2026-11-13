// Hook helper: HEAD of `mobs::ai::burn_undead`, cancellable. A type a mod registered with
// `no_sunburn` is not lit by daylight.
fn armory_lib_sunburn(mob: &crate::mobs::Mob) -> core::ops::ControlFlow<()> {
    if crate::armory_lib_bridge::no_sunburn(&mob.kind.id) {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
