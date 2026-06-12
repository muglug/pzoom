<?php
/**
 * @template E
 * @method E get()
 */
interface I {}

/**
 * @template E
 * @implements I<E>
 */
class A implements I
{
    public function __call(string $name, array $args) {}
}

/**
 * @template E
 * @extends I<E>
 */
interface I2 extends I {}

class B {}

/**
 * @template E
 * @method E get()
 */
class C
{
    public function __call(string $name, array $args) {}
}

/**
 * @template E
 * @extends C<E>
 */
class D extends C {}

/** @var A<B> $a */
$a = new A();
$b = $a->get();

/** @var I2<B> $i */
$c = $i->get();

/** @var D<B> $d */
$d = new D();
$e = $d->get();
