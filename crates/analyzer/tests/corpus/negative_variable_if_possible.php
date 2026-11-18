<?php
// Rule: variable.undefined; deliberate single error: variable_if_possible.
function f($flag) { if ($flag) { $value = 1; } echo $value; }
