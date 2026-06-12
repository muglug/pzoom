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
}

function foo() : void {
    (new A("hello"))->setFoo("goodbye");
}
