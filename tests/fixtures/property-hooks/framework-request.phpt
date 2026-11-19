--TEST--
A request-like constructor, public setter and clone share validation
--FILE--
<?php
class RequestData {
    public string $method {
        set {
            $this->method = strtoupper($value);
        }
    }
    public array $query {
        set(array $parameters) {
            $this->query = $parameters;
        }
    }
    public function __construct(string $method, array $query) {
        $this->method = $method;
        $this->query = $query;
    }
}
$request = new RequestData('get', ['name' => 'x']);
$second = clone($request, ['method' => 'post']);
$second->query = ['name' => 'y'];
echo $request->method, ' ', $request->query['name'], "\n";
echo $second->method, ' ', $second->query['name'], "\n";
--EXPECT--
GET x
POST y
