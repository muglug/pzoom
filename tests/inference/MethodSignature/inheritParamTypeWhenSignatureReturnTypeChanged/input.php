<?php
class A {
    public function __construct(string $s) {}
}

class AChild extends A {}

interface B  {
    /** @param string $data */
    public function create($data): A;
}

class C implements B {
    public function create($data): AChild {
        return new AChild($data);
    }
}
