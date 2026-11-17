--TEST--
app validation rules
--FILE--
<?php
class Validator {
    private $rules = [];
    public function add($field, callable $rule) {
        $this->rules[$field][] = $rule;
    }
    public function validate($data) {
        $errors = [];
        foreach ($this->rules as $field => $rules) {
            foreach ($rules as $rule) {
                try {
                    $rule($data[$field]);
                } catch (RuntimeException $error) {
                    $errors[] = $field . ':' . $error->getMessage();
                }
            }
        }
        return $errors;
    }
}
$validator = new Validator;
$minimum = 3;
$validator->add('name', function ($value) use ($minimum) {
    if (strlen(trim($value)) < $minimum) {
        throw new RuntimeException('short');
    }
});
$validator->add('age', function ($value) {
    if ($value < 18) {
        throw new RuntimeException('young');
    }
});
var_dump($validator->validate(['name' => 'Al', 'age' => 17]));
var_dump($validator->validate(['name' => 'Ada', 'age' => 30]));
--EXPECT--
array(2) {
  [0]=>
  string(10) "name:short"
  [1]=>
  string(9) "age:young"
}
array(0) {
}
