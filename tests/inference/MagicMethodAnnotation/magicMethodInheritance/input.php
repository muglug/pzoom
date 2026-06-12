<?php
/**
 * @method string foo()
 */
interface I {}

/**
 * @method int bar()
 */
class A implements I {}

class B extends A {
    public function __call(string $method, array $args) {}
}

$b = new B();

function consumeString(string $s): void {}
function consumeInt(int $i): void {}

consumeString($b->foo());
consumeInt($b->bar());
