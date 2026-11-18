<?php
// Rule: variable.undefined; deliberate single error: variable_or_possible.
function f($flag) { $flag || ($value = 1); echo $value; }
