use crate::cmd::HooksAction;

pub fn run(action: &HooksAction) {
    match action {
        HooksAction::Install => eprintln!("hooks install: not yet implemented"),
        HooksAction::Remove => eprintln!("hooks remove: not yet implemented"),
    }
}
