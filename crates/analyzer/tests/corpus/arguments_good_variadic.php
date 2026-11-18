<?php
// Rule: arguments.count / argument.unknown; arguments good variadic.
function f($a, ...$rest) {} f(1, 2, 3);
