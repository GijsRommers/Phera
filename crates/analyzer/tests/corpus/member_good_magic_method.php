<?php
// Rule: members / constants / object contexts; member good magic method.
class C { function __call($n, $a) {} function f() { $this->anything(); } }
