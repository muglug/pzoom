<?php
class ParentClass {
    public function __call(string $name, array $args) {}
    public static function __callStatic(string $name, array $args) {}
}

class Provider {
    public function getString() : string {
        return "hello";
    }

    public function setInteger(int $i) : void {}

    public static function getInt() : int {
        return 5;
    }
}

/** @mixin Provider */
class Child extends ParentClass {}

$child = new Child();

$a = $child->getString();
$b = $child::getInt();
