<?php
// Rule: members / constants / object contexts; member good magic property.
class C { function __get($n) {} function f() { echo $this->anything; } }
