<?php
require __DIR__."/../support.php";
$r->setOption(Redis::OPT_SERIALIZER,Redis::SERIALIZER_NONE);
foreach ([null,false,true,42,1.5,"a\0b",["a"=>[1,2]]] as $value) {
check(fn()=>$r->set($p."k",$value));check(fn()=>$r->get($p."k"));
check(fn()=>$r->hSet($p."h","f",$value));check(fn()=>$r->hGet($p."h","f"));}
finish();
