<?php
class BaseClass {
    public const KEYS = [
        "a" => "a",
        "b" => "b",
    ];
}

class ChildClass extends BaseClass {
    public const A = [
        ...parent::KEYS,
        "c" => "c",
    ];
}

$a = ChildClass::A;
