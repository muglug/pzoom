<?php
/** @template T */
abstract class Foo {
    /** @return T */
    abstract public function getValue();
}

/** @extends Foo<int> */
abstract class FooChild extends Foo {}

class F extends FooChild {
    public function getValue() {
        return 10;
    }
}

echo (new F())->getValue();