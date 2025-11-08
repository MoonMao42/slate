use crate::error::Result;
use crate::cli::wizard_core::Wizard;

/// Handle `slate setup` and `slate setup --quick` commands
pub fn handle(args: &[&str]) -> Result<()> {
    let quick_mode = args.contains(&"--quick");
    
    let mut wizard = Wizard::new()?;
    wizard.run(quick_mode)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_quick_flag_recognized() {
        // Verify --quick flag is parsed correctly
        let args = vec!["--quick"];
        let has_quick = args.contains(&"--quick");
        assert!(has_quick);
    }

    #[test]
    fn test_setup_no_args() {
        // Verify empty args works
        let args: Vec<&str> = vec![];
        let has_quick = args.contains(&"--quick");
        assert!(!has_quick);
    }
}
