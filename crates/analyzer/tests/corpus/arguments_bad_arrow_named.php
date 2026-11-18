<?php
// Rule: arguments.count / argument.unknown; arguments bad arrow named.
$f = fn($a = 1) => $a; $f(wrong: 1); // error0
