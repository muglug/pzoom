<?php
/**
 * @psalm-external-mutation-free
 */
final class A {
    private string $foo;

    public function __construct(string $foo) {
        $this->foo = $foo;
    }

    public function setFoo(string $foo) : void {
        $this->foo = $foo;
    }

    public function getFoo() : string {
        return $this->foo;
    }
}

$a = new A("hello");
$a->setFoo($a->getFoo() . "cool");
