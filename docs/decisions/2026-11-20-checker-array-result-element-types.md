# Use implicit mixed for undeclared array result elements

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

At level 10, use implicit mixed for elements of a known function or
method result declared as bare `array`. Array element types resolved
from an inherited PHPDoc return take precedence. Keep the type
unknown when the checker cannot resolve the call or a native stub
lacks its result details.
