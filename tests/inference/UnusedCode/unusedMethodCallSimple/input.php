<?php
final class A {
    private string $foo;

    public function __construct(string $foo) {
        $this->foo = $foo;
    }

    public function getFoo() : string {
        return $this->foo;
    }
}

$a = new A("hello");
$a->getFoo();
