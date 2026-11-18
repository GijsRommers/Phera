<?php
// Rule: members / constants / object contexts; member good dynamic method.
class C { function f($n) { $this->$n(); } }
