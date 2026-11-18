<?php
// Rule: class.notFound; class relative extends.
namespace App;
class C extends namespace\Missing {} // error0
