<?php
/** @psalm-no-seal-methods */
class A {
    public function __call(string $method_name, array $args) : string {
        return "hello";
    }
}

$a = new A;
$s = $a->bar();
