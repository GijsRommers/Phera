<?php
// Rule: class.notFound; class alias implements.
namespace App; use Missing as AliasMissing;
class C implements AliasMissing {} // error0
