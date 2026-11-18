<?php
// Rule: members / constants / object contexts; member good trait consumer.
trait T { function f() { $this->go(); echo $this->p; } } class C { use T; public $p; function go() {} }
