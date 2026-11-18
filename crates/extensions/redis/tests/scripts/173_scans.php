<?php
require __DIR__ . '/../support.php';
$r->hMSet($p.'h',['a'=>'1','b'=>'2']);
$r->sAdd($p.'s','a','b');
$r->zAdd($p.'z',1,'a',2,'b');
foreach (['hscan','sscan','zscan'] as $method) {
    $key = $p . ($method === 'hscan' ? 'h' : ($method === 'sscan' ? 's' : 'z'));
    $iterator = null;
    check(function () use ($r, $method, $key, &$iterator) {
        return $r->$method($key,$iterator,'*',100);
    });
    check(fn()=>$iterator);
    check(function () use ($r, $method, $key, &$iterator) {
        return $r->$method($key,$iterator,'*',100);
    });
}
$r->setOption(Redis::OPT_SCAN,Redis::SCAN_RETRY);
$iterator=null;$found=[];
do { $page=$r->scan($iterator,$p.'*',100);if($page){foreach($page as $key){$found[]=$key;}} }
while($iterator!==0);
sort($found);check(fn()=>$found);
finish();
