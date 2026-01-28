<?php
interface Mock {
    function foo():void;
}
abstract class A {}
class B extends A {}
class BMock extends B implements Mock {
    public function foo(): void {}
}

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
class BTestCase extends ATestCase {}

new BTestCase(new BMock());