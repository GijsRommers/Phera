<?php
require __DIR__."/../support.php";
$r->set($p."k","ab\0cd");
foreach ([0,1,8,-1] as $start) { foreach ([-1,3,15] as $end) {
foreach ([false,true] as $bits) {
check(fn()=>$r->bitcount($p."k",$start,$end,$bits));
check(fn()=>$r->bitpos($p."k",true,$start,$end,$bits));
}}}
finish();
