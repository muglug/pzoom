<?php
/** @psalm-immutable */
class B {
    private ?array $arr = null;

    public function __construct(?array $arr) {
        $this->arr = $arr;
    }

    public function getArray() : ?array {
        return $this->arr;
    }
}

/** @psalm-immutable */
class A {
    public B $b;
    public function __construct(B $b) {
        $this->b = $b;
    }

    /** @psalm-assert-if-true !null $this->b->getarray() */
    public function hasArray() : bool {
        return $this->b->getArray() !== null;
    }
}

function foo(A $a) : void {
    if ($a->hasArray()) {
        echo count($a->b->getArray());
    }
}
