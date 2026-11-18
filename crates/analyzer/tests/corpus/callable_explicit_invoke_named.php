<?php
// Rule: argument.unknown; callable_explicit_invoke_named.
$f = fn($x = 0) => $x;
$f->__invoke(wrong: 1);
