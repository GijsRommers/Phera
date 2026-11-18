<?php
// Rule: members / constants / object contexts; member good static forward.
class C { function go() {} function f() { static::go(); } }
