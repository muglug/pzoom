<?php
class A {}

/**
 * @psalm-template T
 */
class Foo {
    /**
     * @param T $x
     */
    public function bar($x): void { }
}

$afoo = new Foo();
$afoo->bar(new A());