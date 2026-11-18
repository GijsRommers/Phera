<?php
// Rule: members / constants / object contexts; member good trait alias.
trait T { function go() {} } class C { use T { go as alias; } function f() { $this->alias(); } }
