<?php
// Rule: variable.undefined; deliberate single error: variable_switch_possible.
function f($flag) { switch ($flag) { case 0: $value = 1; } echo $value; }
