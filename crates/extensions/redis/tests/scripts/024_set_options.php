<?php
require __DIR__."/../support.php";
foreach ([null,60,["nx"],["xx"],["get"],["keepttl"],["ex"=>60],
["px"=>60000],["exat"=>4102444800],["pxat"=>4102444800000],
["nx","ex"=>60],["xx","get","px"=>60000]] as $options) {
$r->del($p."k"); check(fn()=>$r->set($p."k","first",$options));
check(fn()=>$r->set($p."k","second",$options)); check(fn()=>$r->get($p."k"));
}
finish();
