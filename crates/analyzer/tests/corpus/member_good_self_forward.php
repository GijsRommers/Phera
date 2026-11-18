<?php
// Rule: members / constants / object contexts; member good self forward.
class C { function go() {} function f() { self::go(); } }
