--TEST--
Application class sweep: command-dispatch
--FILE--
<?php
class Commands {
function start(
)
{
return 'started';
}
function stop(
)
{
return 'stopped';
}
}
class Dispatcher {
function __construct(
private Commands $commands)
{
}
function run(
string $method)
: string {
return $this->commands->$method();
}
}
$dispatcher=new Dispatcher(new Commands);
foreach(['start',
'stop',
'start'] as $command){
echo $dispatcher->run($command),
"\n";
}

--EXPECT--
started
stopped
started
