<?php
class A {}
class AChild extends A {}

interface IParent {
    public function get(): A;
}

interface IChild extends IParent {
    /**
     * @psalm-return AChild
     */
    public function get(): A;
}

class Concrete implements IChild {
    public function get(): A {
        return new AChild;
    }
}
