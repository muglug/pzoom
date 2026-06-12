<?php
/** @psalm-immutable */
class B {
    public ?array $arr = null;

    public function __construct(?array $arr) {
        $this->arr = $arr;
    }
}

/** @psalm-immutable */
class A {
    public B $b;
    public function __construct(B $b) {
        $this->b = $b;
    }

    /** @psalm-assert-if-true !null $this->b->arr */
    public function hasArray() : bool {
        return $this->b->arr !== null;
    }
}

function foo(A $a) : void {
    if ($a->hasArray()) {
        echo count($a->b->arr);
    }
}
