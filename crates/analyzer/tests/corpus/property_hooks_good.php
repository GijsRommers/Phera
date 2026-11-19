<?php
// Rule: variable.undefined; property_hooks_good.
interface HookContract {
    public string $label { get; }
}

abstract class HookBase implements HookContract {
    public string $name = 'base';
    abstract public string $label { get; }
}

class HookRecord extends HookBase {
    public string $name = 'child' {
        get => parent::$name::get();
        set { $this->name = trim($value); }
    }
    public string $label {
        get => strtoupper($this->name);
    }
    public string $alias {
        set(string $input) { $this->name = $input; }
    }

    public function __construct(public string $title {
        get => $this->title;
        set => trim($value);
    }) {}
}
