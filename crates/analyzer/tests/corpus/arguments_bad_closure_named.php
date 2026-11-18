<?php
// Rule: arguments.count / argument.unknown; arguments bad closure named.
$f = function ($a = 1) {}; $f(wrong: 1); // error0
