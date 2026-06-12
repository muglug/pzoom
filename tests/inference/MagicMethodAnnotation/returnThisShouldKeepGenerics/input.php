<?php
/**
 * @template E
 * @method $this foo()
 */
class A
{
    public function __call(string $name, array $args) {}
}

/**
 * @template E
 * @method $this foo()
 */
interface I {}

class B {}

/** @var A<B> $a */
$a = new A();
$b = $a->foo();

/** @var I<B> $i */
$c = $i->foo();
