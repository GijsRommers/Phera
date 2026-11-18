<?php
// Rule: members / constants / object contexts; member good dynamic property.
class C { function f($n) { echo $this->$n; } }
