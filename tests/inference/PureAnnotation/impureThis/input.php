<?php
class A {
    public int $a = 5;

    /**
     * @psalm-pure
     */
    public function foo() : self {
        return $this;
    }
}
