<?php
// Rule: members / constants / object contexts; member good parent forward.
class P { function go() {} } class C extends P { function f() { parent::go(); } }
