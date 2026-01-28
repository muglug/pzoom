<?php
/** @template T */
class Foo {
    /** @param \Closure():T $closure */
    public function __construct($closure) {}
}
class Bar {
    /** @var Foo<array> */
    private $FooArray;
    public function __construct() {
        $this->FooArray = new Foo(function(): array { return []; });
    }
}