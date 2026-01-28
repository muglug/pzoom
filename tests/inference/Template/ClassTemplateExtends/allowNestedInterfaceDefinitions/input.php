<?php
class A {}

/** @template T as object */
interface Container {
    /** @return T */
    public function get();
}

/** @extends Container<A> */
interface AContainer extends Container {
    public function get(): A;
}

interface AContainer2 extends AContainer {}

class ConcreteAContainer implements AContainer2 {
    public function get(): A {
        return new A();
    }
}