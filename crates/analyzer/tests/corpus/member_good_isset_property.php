<?php
// Rule: members / constants / object contexts; member good isset property.
class C {} $c = new C; if (isset($c->foo)) {}
