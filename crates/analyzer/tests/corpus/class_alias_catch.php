<?php
// Rule: class.notFound; class alias catch.
namespace App; use Missing as AliasMissing;
try {} catch (AliasMissing $e) {} // error0
