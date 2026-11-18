<?php
require __DIR__."/../support.php";
check(fn()=>$r->httl($p."k",["a","missing"]));
$r->hSet($p."k","a","12");check(fn()=>$r->httl($p."k",["a","missing"]));
$r->hpexpireat($p."k",4102444800000,["a"]);
check(fn()=>$r->httl($p."k",["missing"]));
finish();
