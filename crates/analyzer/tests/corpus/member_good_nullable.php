<?php
// Rule: members / constants / object contexts; member good nullable.
class C {} function f(?C $c) { $c?->anything(); }
