<?php
final class A {
    public string $a;

    public function __construct() {
        $this->a = "hello";
    }
}

$foo = new A();
echo $foo->a;
