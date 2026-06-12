<?php
/** @template T */
class A {
    /** @var T */
    public $value;

    /** @param T $value */
    public function __construct($value) {
        $this->value = $value;
    }
}

/**
 * @psalm-type Foo=string
 * @extends A<Foo>
 */
class C extends A {}

$instance = new C("hello");
$output = $instance->value;
