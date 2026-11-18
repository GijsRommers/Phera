<?php
// Rule: members / constants / object contexts; member good inherited property.
class P { public $p; } class C extends P { function f() { echo $this->p; } }
