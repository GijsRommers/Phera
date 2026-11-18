<?php
// Rule: variable.undefined; deliberate single error: variable_foreach_possible.
function f($items) { foreach ($items as $value) {} echo $value; }
