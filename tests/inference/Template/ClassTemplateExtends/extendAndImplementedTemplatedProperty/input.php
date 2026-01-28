<?php
interface Mock {}
abstract class A {}
class B extends A {}
class BMock extends B {}

/** @template T of A */
abstract class ATestCase {
    /** @var T */
    protected $foo;

    /** @param T $foo */
    public function __construct(A $foo) {
        $this->foo = $foo;
    }
}

/** @extends ATestCase<B> */
class BTestCase extends ATestCase {
    public function getFoo(): B {
        return $this->foo;
    }
}

new BTestCase(new BMock());