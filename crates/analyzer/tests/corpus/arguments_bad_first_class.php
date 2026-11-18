<?php
// Rule: arguments.count / argument.unknown; arguments bad first class.
function f($a = 1) {} $f = f(...); $f(wrong: 1); // error0
