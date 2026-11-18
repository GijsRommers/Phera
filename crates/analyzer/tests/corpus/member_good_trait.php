<?php
// Rule: members / constants / object contexts; member good trait.
trait T { function go() {} } class C { use T; function f() { $this->go(); } }
