<?php
require __DIR__ . '/../support.php';
$library='phera_fn_'.str_replace(':','_',$p);
$fn=$library.'echo';
$code="#!lua name=$library\nredis.register_function{function_name='$fn', callback=function(keys,args) return {redis.call('GET',keys[1]),args[1]} end, flags={'no-writes'}}";
try {
    check(fn()=>$r->function('LOAD',$code));
    $r->setOption(Redis::OPT_PREFIX,$p);$r->set('value','stored');
    check(fn()=>$r->fcall($fn,['value'],["a\0b"]));
    check(fn()=>$r->fcall_ro($fn,['value'],['readonly']));
    check(fn()=>$r->function('LIST','LIBRARYNAME',$library));
    foreach (['multi','pipeline'] as $mode) {
        $r->$mode();check(fn()=>$r->fcall($fn,['value'],['queued']));
        check(fn()=>$r->exec());
    }
    check(fn()=>$r->function('LOAD','REPLACE',$code));
} finally {
    $r->function('DELETE',$library);$r->setOption(Redis::OPT_PREFIX,'');
}
finish();
