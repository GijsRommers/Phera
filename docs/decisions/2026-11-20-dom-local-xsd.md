# Validate DOM documents against local XSDs

- Date: 2026-11-20
- Area: DOM
- Decided by: Design

## Decision

`DOMDocument::schemaValidate` and `schemaValidateSource` validate common XSD 1.0
declarations against the DOM arena. Explicit XSD includes and imports can open
local regular files. Each validation bounds the number of files, total schema
bytes, and work spent matching content models. XML parsing and XSD loading
leave external entities inert. XSD loading refuses doctypes, network locations,
and schema redefinition.

## Why

Symfony's XML configuration loader uses inline schemas that import packaged
XSD files. Explicit validation needs those imports to resolve. Local file input
allows that workflow without changing the document parser's entity policy.

## Turned down

Linking libxml2, following schema locations from the instance document, and
fetching schemas over the network.
