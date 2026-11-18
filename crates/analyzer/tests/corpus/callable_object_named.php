<?php
// Rule: argument.unknown; callable_object_named.
class C { function __invoke($x = 0) {} }
$c = new C;
$c(wrong: 1);
