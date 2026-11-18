<?php
// Rule: references / scope / traits; eval_erases_types.
class C { function f($x) {} } function g($code) { $x = new C; eval($code); $x->f(); }
