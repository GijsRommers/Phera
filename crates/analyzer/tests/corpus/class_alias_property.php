<?php
// Rule: class.notFound; class alias property.
namespace App; use Missing as AliasMissing;
class C { public AliasMissing $p; } // error0
