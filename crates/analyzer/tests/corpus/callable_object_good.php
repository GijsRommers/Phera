<?php
// Rule: argument.unknown; callable_object_good.
class C { function __invoke($x) {} }
$c = new C;
$c(x: 1);
