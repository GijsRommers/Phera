<?php
// Rule: variable.undefined; a stored closure may acquire an object before invocation.
class Box { private $x = 7; }
$f = function () { return $this->x; };
$g = $f->bindTo(new Box, Box::class);
echo $g();
