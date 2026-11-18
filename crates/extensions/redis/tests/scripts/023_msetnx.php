<?php
require __DIR__."/../support.php";
foreach (["", "ab\0cd", "é", str_repeat("x",65536)] as $value) {
$r->del($p."k",$p."b"); check(fn()=>$r->msetnx([$p."k"=>$value,$p."b"=>$value]));
check(fn()=>strlen($r->get($p."k"))); check(fn()=>$r->msetnx([$p."k"=>$value,$p."b"=>$value]));
}
finish();
