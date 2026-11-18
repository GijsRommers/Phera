<?php
// Rule: class.missingMethod; member bad missing trait contract.
trait T { abstract function go(); } class C { use T; } // error0
