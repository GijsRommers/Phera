<?php
// Rule: members / constants / object contexts; member good inherited method.
class P { function go() {} } class C extends P { function f() { $this->go(); } }
