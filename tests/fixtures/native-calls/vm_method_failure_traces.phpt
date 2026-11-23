--TEST--
Generator and Fiber engine errors keep instance and static method frames
--INI--
zend.exception_ignore_args=0
--FILE--
<?php
function probe(string $label, Closure $call): void {
    try {
        $call();
        echo "$label | returned\n";
    } catch (Throwable $error) {
        $frame = $error->getTrace()[0] ?? [];
        $first = explode("\n", $error->getTraceAsString())[0];
        $rendered = str_contains($first, '): ') ? explode('): ', $first, 2)[1] : $first;
        if (!isset($frame['class'])) {
            $rendered = 'caller frame';
        }
        printf("%s | %s | %s%s%s (%d args, object=%d) | %s\n", $label, get_class($error),
            $frame['class'] ?? '', $frame['type'] ?? '',
            isset($frame['class']) ? $frame['function'] : '-',
            count($frame['args'] ?? []), isset($frame['object']), $rendered);
    }
}
function yields() { yield 1; yield 2; }
probe('Generator rewind', function () { $g = yields(); $g->next(); $g->rewind(); });
probe('Generator getReturn', fn() => yields()->getReturn());
probe('Fiber start', function () { $f = new Fiber(fn() => 1); $f->start(); $f->start(2); });
probe('Fiber resume', fn() => (new Fiber(fn() => 1))->resume(42));
probe('Fiber throw', fn() => (new Fiber(fn() => 1))->throw(new Exception()));
probe('Fiber constructor', function () { $f = new Fiber(fn() => 1); $f->__construct(fn() => 2); });
probe('Fiber getCurrent', fn() => Fiber::getCurrent(2));
probe('Fiber suspend', fn() => Fiber::suspend('value'));
probe('ReflectionFiber getExecutingFile', fn() => (new ReflectionFiber(new Fiber(fn() => 1)))->getExecutingFile());
probe('ReflectionFiber getExecutingLine', fn() => (new ReflectionFiber(new Fiber(fn() => 1)))->getExecutingLine());
probe('ReflectionFiber getTrace', fn() => (new ReflectionFiber(new Fiber(fn() => 1)))->getTrace());
--EXPECT--
Generator rewind | Exception | Generator->rewind (0 args, object=0) | Generator->rewind()
Generator getReturn | Exception | Generator->getReturn (0 args, object=0) | Generator->getReturn()
Fiber start | FiberError | Fiber->start (1 args, object=0) | Fiber->start(2)
Fiber resume | FiberError | Fiber->resume (1 args, object=0) | Fiber->resume(42)
Fiber throw | FiberError | Fiber->throw (1 args, object=0) | Fiber->throw(Object(Exception))
Fiber constructor | FiberError | Fiber->__construct (1 args, object=0) | Fiber->__construct(Object(Closure))
Fiber getCurrent | ArgumentCountError | Fiber::getCurrent (1 args, object=0) | Fiber::getCurrent(2)
Fiber suspend | FiberError | Fiber::suspend (1 args, object=0) | Fiber::suspend('value')
ReflectionFiber getExecutingFile | Error | ReflectionFiber->getExecutingFile (0 args, object=0) | ReflectionFiber->getExecutingFile()
ReflectionFiber getExecutingLine | Error | ReflectionFiber->getExecutingLine (0 args, object=0) | ReflectionFiber->getExecutingLine()
ReflectionFiber getTrace | Error | ReflectionFiber->getTrace (0 args, object=0) | ReflectionFiber->getTrace()
