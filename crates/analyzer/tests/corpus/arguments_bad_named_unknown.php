<?php
// Rule: arguments.count / argument.unknown; arguments bad named unknown.
function f($a = 1) {} f(wrong: 1); // error0
