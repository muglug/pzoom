<?php
class Foo {}
class Bar {}

class B {
    /** @return array<int, Foo> */
    public function getFoos() : array {
        return [new Foo()];
    }
}

class A extends B {
    public function getFoos() : array {
        return [new Bar()];
    }
}
