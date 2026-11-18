<?php
// Rule: members / constants / object contexts; member good inherited constant.
class P { const A = 1; } class C extends P { function f() { echo self::A; } }
