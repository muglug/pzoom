<?php
/** @template T */
interface Foo {
    /** @return T */
    public function getValue();
}

/** @extends Foo<int> */
interface FooChild extends Foo {}

class F implements FooChild {
    public function getValue() {
        return 10;
    }
}

echo (new F())->getValue();