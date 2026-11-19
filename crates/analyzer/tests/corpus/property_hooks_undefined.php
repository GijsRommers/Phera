<?php
class BrokenHookRecord {
    public string $name {
        get { echo $missing; return $this->name; }
        set { $this->name = $value; }
    }

    public function __construct(public string $title {
        get => $this->title;
        set { echo $typo; $this->title = $value; }
    }) {}
}
