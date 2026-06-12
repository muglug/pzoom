<?php
final class A {
    public string $a = "hello";
    public string $b = "world";

    public function __construct() {
        $this->a = "hello";
        $this->b = "world";
    }
}

$foo = new A();
echo $foo->a;
