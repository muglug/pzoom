<?php
class A {}
class B {}
class C {}
class D {}

/**
 * @template T as object
 */
class Foo {
    /** @var T::class */
    public $T;

    /**
     * @param class-string<T> $T
     */
    public function __construct(string $T) {
        $this->T = $T;
    }

    /**
     * @return T
     */
    public function bar() {
        $t = $this->T;
        return new $t();
    }
}

$at = "A";

/**
 * @var Foo<A>
 * @psalm-suppress ArgumentTypeCoercion
 */
$afoo = new Foo($at);
$afoo_bar = $afoo->bar();

$bfoo = new Foo(B::class);
$bfoo_bar = $bfoo->bar();

// this shouldn’t cause a problem as it’s a docbblock type
if (!($bfoo_bar instanceof B)) {}

$c = C::class;
$cfoo = new Foo($c);
$cfoo_bar = $cfoo->bar();