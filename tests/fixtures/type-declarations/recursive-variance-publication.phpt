--TEST--
A recursive autoload delays publication until inheritance checks finish
--FILE--
<?php
class ServiceBase {
    public function primary(): Service { throw new LogicException; }
    public function fallback(): Service { throw new LogicException; }
}

spl_autoload_register(function ($name) {
    echo 'load ', $name, "\n";
    switch ($name) {
        case 'Service':
            class Service extends ServiceBase {
                public function primary(): PrimaryService { throw new LogicException; }
                public function fallback(): FallbackService { throw new LogicException; }
            }
            break;
        case 'PrimaryService':
            var_dump(class_exists(Service::class, false));
            class PrimaryService extends Service {}
            break;
        case 'FallbackService':
            class FallbackService extends Service {}
            break;
    }
    echo 'ready ', get_class(new $name), "\n";
});
echo 'result ', get_class(new Service), "\n";
--EXPECT--
result load Service
load PrimaryService
bool(false)
load FallbackService
ready FallbackService
ready PrimaryService
ready Service
Service
