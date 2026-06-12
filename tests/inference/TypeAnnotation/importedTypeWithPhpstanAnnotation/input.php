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
 * @phpstan-type Foo=string
 */
class B {}

/**
 * @phpstan-import-type Foo from B
 * @phpstan-type Baz=Foo
 *
 * @extends A<Baz>
 */
class C extends A {}

$instance = new C("hello");
$output = $instance->value;
