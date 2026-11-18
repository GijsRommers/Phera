<?php
// Rule: variable.undefined; deliberate single error: variable_compact_read.
function f() { return compact("unavailable"); }
