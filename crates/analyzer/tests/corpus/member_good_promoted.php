<?php
// Rule: members / constants / object contexts; member good promoted.
class C { function __construct(public int $p) {} function f() { return $this->p; } }
