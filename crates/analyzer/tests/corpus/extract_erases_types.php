<?php
// Rule: references / scope / traits; extract_erases_types.
class C { function f($x) {} } function g($input) { $x = new C; extract($input); $x->f(); }
