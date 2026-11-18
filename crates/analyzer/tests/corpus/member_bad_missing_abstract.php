<?php
// Rule: class.missingMethod; member bad missing abstract.
abstract class P { abstract function go(); } class C extends P {} // error0
