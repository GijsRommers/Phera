<?php
// Rule: class.notFound; class relative implements.
namespace App;
class C implements namespace\Missing {} // error0
