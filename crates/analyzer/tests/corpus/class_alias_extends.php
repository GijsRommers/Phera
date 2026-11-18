<?php
// Rule: class.notFound; class alias extends.
namespace App; use Missing as AliasMissing;
class C extends AliasMissing {} // error0
