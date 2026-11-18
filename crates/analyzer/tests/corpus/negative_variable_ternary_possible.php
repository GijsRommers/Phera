<?php
// Rule: variable.undefined; deliberate single error: variable_ternary_possible.
function f($flag) { $flag ? ($value = 1) : null; echo $value; }
