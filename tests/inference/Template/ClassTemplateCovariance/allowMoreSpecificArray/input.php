<?php
/** @template-covariant T */
class Foo {
    /** @param \Closure():T $closure */
    public function __construct($closure) {}
}

class Bar {
    /** @var Foo<array> */
    private $arrayOfFoo;

    public function __construct() {
        $this->arrayOfFoo = new Foo(function(): array { return ["foo" => "bar"]; });
    }
}