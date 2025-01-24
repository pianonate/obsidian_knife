#[derive(Debug, PartialEq)]
pub enum CodeBlockDelimiter {
    Backtick,
    TripleBacktick,
}

impl TryFrom<&str> for CodeBlockDelimiter {
    type Error = (); // Using unit type for error since we don't care if it fails

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if s.trim().starts_with("```") {
            Ok(CodeBlockDelimiter::TripleBacktick)
        } else {
            Err(())
        }
    }
}

impl TryFrom<char> for CodeBlockDelimiter {
    type Error = ();

    fn try_from(c: char) -> Result<Self, Self::Error> {
        match c {
            '`' => Ok(CodeBlockDelimiter::Backtick),
            _ => Err(()),
        }
    }
}

#[derive(Debug, PartialEq)]
enum BlockLocation {
    Outside,
    Inside,
    OnClosingDelimiter,
}

pub trait BlockDelimiter {
    fn delimiter_type(&self) -> CodeBlockDelimiter;
}

pub struct TripleBacktickDelimiter;
impl BlockDelimiter for TripleBacktickDelimiter {
    fn delimiter_type(&self) -> CodeBlockDelimiter {
        CodeBlockDelimiter::TripleBacktick
    }
}

pub struct SingleBacktickDelimiter;
impl BlockDelimiter for SingleBacktickDelimiter {
    fn delimiter_type(&self) -> CodeBlockDelimiter {
        CodeBlockDelimiter::Backtick
    }
}

#[derive(Debug)]
pub struct BlockTracker<D: BlockDelimiter> {
    location: BlockLocation,
    delimiter: D,
}

impl<D: BlockDelimiter> BlockTracker<D> {
    pub fn new_with_delimiter(delimiter: D) -> Self {
        Self {
            location: BlockLocation::Outside,
            delimiter,
        }
    }

    pub fn update<T>(&mut self, content: T)
    where
        T: TryInto<CodeBlockDelimiter>,
    {
        if let Ok(delimiter) = content.try_into() {
            if delimiter == self.delimiter.delimiter_type() {
                match self.location {
                    BlockLocation::Inside => {
                        self.location = BlockLocation::OnClosingDelimiter;
                    }
                    BlockLocation::Outside => {
                        self.location = BlockLocation::Inside;
                    }
                    BlockLocation::OnClosingDelimiter => {
                        self.location = BlockLocation::Inside;
                    }
                }
            }
        } else if self.location == BlockLocation::OnClosingDelimiter {
            self.location = BlockLocation::Outside;
        }
    }

    pub fn should_skip(&self) -> bool {
        matches!(
            self.location,
            BlockLocation::Inside | BlockLocation::OnClosingDelimiter
        )
    }

    pub fn is_inside(&self) -> bool {
        self.location == BlockLocation::Inside
    }
}

pub type CodeBlockExcluder = BlockTracker<TripleBacktickDelimiter>;
pub type InlineCodeExcluder = BlockTracker<SingleBacktickDelimiter>;

impl CodeBlockExcluder {
    pub fn new() -> Self {
        Self::new_with_delimiter(TripleBacktickDelimiter)
    }
}

impl InlineCodeExcluder {
    pub fn new() -> Self {
        Self::new_with_delimiter(SingleBacktickDelimiter)
    }
}

#[test]
fn test_code_block_tracking() {
    let mut tracker = CodeBlockExcluder::new();

    // Initial state
    assert!(!tracker.should_skip(), "Initial state should not skip");

    tracker.update("```rust");
    assert!(tracker.should_skip(), "Should skip inside code block");
    tracker.update("let x = 42;");
    assert!(tracker.should_skip(), "Should still be in code block");
    tracker.update("```");
    assert!(
        tracker.should_skip(),
        "Should skip while processing closing delimiter"
    );

    tracker.update("next line"); // This moves us to Outside
    assert!(!tracker.should_skip(), "Should not skip after code block");

    // Regular content
    tracker.update("Regular text");
    assert!(!tracker.should_skip(), "Should not be in code block");

    // Nested code blocks (treated as toggles)
    tracker.update("```python");
    assert!(tracker.should_skip(), "Should skip in second code block");
    tracker.update("print('hello')");
    tracker.update("```");
    assert!(tracker.should_skip(), "Should skip after second block");

    // immediately following with another code block opening
    tracker.update("```");
    assert!(
        tracker.should_skip(),
        "Should skip after opening another code block right after the last one"
    );
}

#[test]
fn test_inline_code_tracking() {
    let mut tracker = InlineCodeExcluder::new();

    // Initial state
    assert!(!tracker.should_skip(), "Initial state should not skip");

    tracker.update('`');
    assert!(
        tracker.should_skip(),
        "Should skip opening inline code block"
    );

    tracker.update('a');
    assert!(tracker.should_skip(), "should skip inside code block");

    tracker.update('`');
    assert!(
        tracker.should_skip(),
        "Should skip closing inline code block"
    );

    tracker.update('b');
    assert!(
        !tracker.should_skip(),
        "Should not skip regular text after an inline code block"
    );
}
