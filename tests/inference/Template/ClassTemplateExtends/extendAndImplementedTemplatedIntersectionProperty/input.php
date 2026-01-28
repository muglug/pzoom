<?php
interface Mock {
    function foo():void;
}
abstract class A {}
class B extends A {}

/** @template T of A */
abstract class ATestCase {
    /** @var T&Mock */
    protected Mock $obj;

    /** @param T&Mock $obj */
    public function __construct(Mock $obj) {
        $this->obj = $obj;
    }
}

/** @extends ATestCase<B> */
class BTestCase extends ATestCase {
    public function getFoo(): void {
        $this->obj->foo();
    }
}