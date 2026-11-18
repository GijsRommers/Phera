<?php
// Rule: class.notFound; class relative property.
namespace App;
class C { public namespace\Missing $p; } // error0
