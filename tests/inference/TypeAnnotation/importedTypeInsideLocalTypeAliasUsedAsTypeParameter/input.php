<?php
/** @template T */
abstract class A {
    /** @var T */
    public $value;

    /** @param T $value */
    public function __construct($value) {
        $this->value = $value;
    }
}

/**
 * @psalm-type Foo=string
 */
class B {}

/**
 * @psalm-import-type Foo from B
 * @psalm-type Baz=Foo
 *
 * @extends A<Baz>
 */
class C extends A {}

$instance = new C("hello");
$output = $instance->value;
