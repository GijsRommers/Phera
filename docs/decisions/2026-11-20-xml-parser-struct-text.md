# Apply XMLParser whitespace filtering to complete struct fields

- Date: 2026-11-20
- Area: Compatibility
- Decided by: Design

## Decision

Keep text from adjacent parser events together until building the result of
`xml_parse_into_struct()`. With `XML_OPTION_SKIP_WHITE`, drop a field only when its complete
value is XML whitespace. Preserve a callback's replacement of an output argument.

## Why

Token boundaries can fall between a character reference and a whitespace segment in the middle
of the same text. Discarding the segment changes the decoded value of the element.

## Turned down

Dropping whitespace on each tokenizer event and overwriting both outputs after every parse.
