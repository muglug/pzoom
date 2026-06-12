<?php
class A {}
class B extends A {}

class X {
    /**
     * @param B $class
     */
    public function boo(A $class): void {}
}

class Y extends X {
    /**
     * @inheritdoc
     * @param A $class
     */
    public function boo(A $class): void {}
}

class Z extends X {
    /**
     * @inheritDoc
     * @param A $class
     */
    public function boo(A $class): void {}
}

(new Y())->boo(new A());
(new Z())->boo(new A());
