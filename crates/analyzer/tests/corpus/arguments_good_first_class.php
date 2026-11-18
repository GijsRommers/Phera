<?php
// Rule: arguments.count / argument.unknown; arguments good first class.
function f($a) {} $c = f(...); $c(1);
