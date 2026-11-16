--TEST--
Application class sweep: constructor-di
--FILE--
<?php
interface Clock {
function now(
)
: int;
}
 class FixedClock implements Clock {
function __construct(
private int $time)
{
}
 function now(
)
: int {
return $this->time;
}
}
 class Service {
function __construct(
private Clock $clock)
{
}
 function expires(int $duration): int {
return $this->clock->now()+$duration;
}
}
$service=new Service(new FixedClock(100));
echo $service->expires(25),
"\n",
$service->expires(50),
"\n";

--EXPECT--
125
150
