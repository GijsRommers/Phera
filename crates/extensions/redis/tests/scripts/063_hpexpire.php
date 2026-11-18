<?php
require __DIR__."/../support.php";
foreach ([null,"NX","XX","GT","LT"] as $mode) {
$r->del($p."k");$r->hSet($p."k","a","12");
check(fn()=>$r->hpexpire($p."k",4102444800000,["a","missing"],$mode));
check(fn()=>$r->hpexpire($p."k",4102444800000,["a"],$mode));}
finish();
