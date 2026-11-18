<?php
// Rule: arguments.count / argument.unknown; arguments bad named missing.
function f($a, $b = 1) {} f(b: 2); // error0
