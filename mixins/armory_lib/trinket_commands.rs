// Hook helper: native_register_commands (HEAD). /trinkets, when a mod declares the slots.
fn armory_lib_trinket_commands(commands: &mut Vec<horizon_client::mod_api::NativeCommand>) {
    crate::game::armory_lib_trinkets::on_register_commands(commands);
}
