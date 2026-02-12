/// Validates that a string is a valid identifier.
pub fn validate_identifier(input: &str) -> bool {
    if input.is_empty() {
        return false;
    }
    let first = input.chars().next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }
    input.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Validates that a port number is within the valid range.
pub fn validate_port(port: u16) -> bool {
    port > 0 && port <= 65535
}

/// A configurable validator that checks multiple rules.
pub struct RuleValidator {
    rules: Vec<Box<dyn Fn(&str) -> bool>>,
}

impl RuleValidator {
    pub fn new() -> Self {
        RuleValidator { rules: Vec::new() }
    }

    pub fn add_rule<F: Fn(&str) -> bool + 'static>(&mut self, rule: F) {
        self.rules.push(Box::new(rule));
    }

    pub fn validate(&self, input: &str) -> bool {
        self.rules.iter().all(|rule| rule(input))
    }
}
