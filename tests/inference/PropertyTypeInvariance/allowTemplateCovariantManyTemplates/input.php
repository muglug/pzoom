<?php
class A {}
class B extends A {}
class C extends B {}

/**
 * @template Ta
 * @template Tb
 * @template-covariant Tc
 * @template Td
 */
class Foo {
    /** @var Ta|null */
    public $a;

    /** @var Tb|null */
    public $b;

    /** @var Tc|null */
    public $c;

    /** @var Td|null */
    public $d;
}

/**
 * @template Ta
 * @template Tb
 * @template-covariant Tc
 * @template Td
 * @extends Foo<Ta, Tb, Tc, Td>
 */
class Bar extends Foo {}

/**
 * @template Ta
 * @template Tb
 * @template-covariant Tc
 * @template Td
 * @extends Bar<A, B, A, C>
 */
class Baz extends Bar {
    /** @var A|null */
    public $a;

    /** @var B|null */
    public $b;

    /** @var C|null */
    public $c;

    /** @var C|null */
    public $d;
}
