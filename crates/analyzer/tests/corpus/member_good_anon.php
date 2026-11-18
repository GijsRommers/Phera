<?php
// Rule: members / constants / object contexts; member good anon.
new class { public $p; function f() { echo $this->p; } };
