<?php
require __DIR__."/../support.php";
foreach ([Redis::OPT_PREFIX,Redis::OPT_SERIALIZER,Redis::OPT_READ_TIMEOUT,
Redis::OPT_SCAN,Redis::OPT_NULL_MULTIBULK_AS_NULL,Redis::OPT_REPLY_LITERAL,Redis::OPT_COMPRESSION] as $option) {
check(fn()=>$r->getOption($option));}
check(fn()=>$r->setOption(Redis::OPT_PREFIX,$p));check(fn()=>$r->set("key","v"));
check(fn()=>$r->get("key"));check(fn()=>$r->_prefix("key"));
finish();
