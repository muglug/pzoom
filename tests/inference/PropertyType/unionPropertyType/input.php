<?php
class A {
    public string|int $i;

    public function __construct() {
        $this->i = 5;
        $this->i = "hello";
    }
}

$a = new A();

if ($a->i === 3) {}
if ($a->i === "foo") {}
