<?php
// Rule: variable.undefined; deliberate single error: variable_while_possible.
function f($flag) { while ($flag) { $value = 1; break; } echo $value; }
