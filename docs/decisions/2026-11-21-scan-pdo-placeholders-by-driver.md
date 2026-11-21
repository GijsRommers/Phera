# Scan PDO placeholders using each database's syntax

- Date: 2026-11-21
- Area: PDO
- Decided by: Design

The shared placeholder scanner used to treat a backslash as an escape in
every quoted region. It could mistake the boundary of a PostgreSQL
standard string and interpolate a bound value as SQL.

PostgreSQL and MySQL now identify their own quoted regions and comments.
A shared renderer rewrites only the placeholder spans returned by the
driver's scanner. PostgreSQL also turns an escaped question mark into its
literal operator form. MySQL applies its own rules for backticks, hash
comments, and the whitespace after a line comment opener. Native SQLite
owns bracket and backtick lexing and returns the parameter slots it parsed
when preparing the statement. PDO uses those slots directly.

A backslash adjacent to a quote can change which spans are parameters under
a nondefault string mode. Only templates with that pattern read the live
mode during prepare. Emulated execution checks those templates again before
interpolating a value. A changed mode that moves a placeholder returns a
binding error. Native execution sends parameter values separately.

Run the same quoting, comment, and binding cases on PHP and Phera.
