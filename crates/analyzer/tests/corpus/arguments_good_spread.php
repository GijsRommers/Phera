<?php
// Rule: arguments.count / argument.unknown; arguments good spread.
function f($a, $b) {} function g($args) { f(...$args); }
