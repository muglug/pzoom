<?php
class A {
    /**
     * @var string
     */
    public $s;

    public function __construct(string $s) {
        $this->s = $s;
    }
}
class B extends A {}
class C extends B {}
